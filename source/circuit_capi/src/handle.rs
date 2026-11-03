// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ops::{Deref, DerefMut},
    ptr,
    rc::{Rc, Weak},
};

use crate::{QDK_ACTIVE, QDK_BORROWED, QDK_IN_USE, QDK_POISONED, QDK_UNFINISHED, boundary::Error};

pub(crate) struct Lifetime {
    active: Cell<bool>,
    pub poison: Rc<Cell<bool>>,
    dependents: Cell<usize>,
    pub borrowed: Cell<bool>,
    pub unfinished: Cell<bool>,
    callback_scope: RefCell<Weak<CallbackChildren>>,
}

struct ScopedChild {
    address: Rc<Cell<*mut ()>>,
    destroy: unsafe fn(*mut ()),
}

pub(crate) struct CallbackChildren {
    children: RefCell<Vec<ScopedChild>>,
    poison: Rc<Cell<bool>>,
}

impl Drop for CallbackChildren {
    fn drop(&mut self) {
        self.poison.set(true);
        // A dependent is published only after its parents. Explicit release
        // clears its address; reverse publication order is therefore child-first.
        for child in self.children.get_mut().iter().rev() {
            let address = child.address.replace(ptr::null_mut());
            if !address.is_null() {
                // SAFETY: publish registered this allocation and matching destructor.
                // A non-NULL address means no explicit release has consumed it.
                unsafe { (child.destroy)(address) };
            }
        }
    }
}

pub(crate) fn publish<T, U>(value: Box<T>, handle: impl Fn(&T) -> &Handle<U>) -> *mut T {
    let scope = handle(&value).lifetime.callback_scope.borrow().upgrade();
    let Some(scope) = scope else {
        return Box::into_raw(value);
    };
    let address = Rc::new(Cell::new(ptr::null_mut()));
    *handle(&value).callback_address.borrow_mut() = Some(Rc::clone(&address));
    scope.children.borrow_mut().push(ScopedChild {
        address: Rc::clone(&address),
        destroy: destroy_child::<T>,
    });
    let value = Box::into_raw(value);
    address.set(value.cast());
    value
}

unsafe fn destroy_child<T>(address: *mut ()) {
    // SAFETY: only publish's matching typed allocation reaches this destructor.
    unsafe { drop(Box::from_raw(address.cast::<T>())) };
}

/// Retains metadata only, never the parent's native value. Checked C release
/// keeps the parent allocation alive until every registration is removed.
struct Registration(Rc<Lifetime>);

impl Registration {
    fn new(parent: &Rc<Lifetime>) -> Result<Self, Error> {
        let count = parent
            .dependents
            .get()
            .checked_add(1)
            .ok_or_else(|| Error::invalid("too many dependent handles"))?;
        parent.dependents.set(count);
        Ok(Self(Rc::clone(parent)))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.0.dependents.set(self.0.dependents.get() - 1);
    }
}

#[cfg(test)]
thread_local! {
    static LIVE_HANDLES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn live_handles() -> usize {
    LIVE_HANDLES.get()
}

/// Interior state is checked before forming a mutable reference, including
/// reentrant calls from foreign callbacks. Concurrent access is forbidden.
pub(crate) struct Handle<T> {
    value: UnsafeCell<T>,
    pub lifetime: Rc<Lifetime>,
    _parents: Vec<Registration>,
    callback_address: RefCell<Option<Rc<Cell<*mut ()>>>>,
}

impl<T> Handle<T> {
    pub fn new(value: T) -> Self {
        Self::with_lifetime(value, Rc::new(Cell::new(false)), Vec::new())
    }

    fn with_lifetime(value: T, poison: Rc<Cell<bool>>, parents: Vec<Registration>) -> Self {
        #[cfg(test)]
        LIVE_HANDLES.set(LIVE_HANDLES.get() + 1);
        Self {
            value: UnsafeCell::new(value),
            lifetime: Rc::new(Lifetime {
                active: Cell::new(false),
                poison,
                dependents: Cell::new(0),
                borrowed: Cell::new(false),
                unfinished: Cell::new(false),
                callback_scope: RefCell::new(Weak::new()),
            }),
            _parents: parents,
            callback_address: RefCell::new(None),
        }
    }

    pub fn child(
        value: T,
        root: &Rc<Lifetime>,
        parent: Option<&Rc<Lifetime>>,
    ) -> Result<Self, Error> {
        if root.poison.get() {
            return Err(Error::new(QDK_POISONED, "owner is poisoned"));
        }
        let mut registrations = vec![Registration::new(root)?];
        if let Some(parent) = parent {
            if !Rc::ptr_eq(&root.poison, &parent.poison) {
                return Err(Error::invalid("dependent parent belongs to another owner"));
            }
            if !Rc::ptr_eq(root, parent) {
                registrations.push(Registration::new(parent)?);
            }
        }
        let child = Self::with_lifetime(value, Rc::clone(&root.poison), registrations);
        *child.lifetime.callback_scope.borrow_mut() = root.callback_scope.borrow().clone();
        Ok(child)
    }

    pub fn callback_children(&self) -> Rc<CallbackChildren> {
        let scope = Rc::new(CallbackChildren {
            children: RefCell::new(Vec::new()),
            poison: Rc::clone(&self.lifetime.poison),
        });
        *self.lifetime.callback_scope.borrow_mut() = Rc::downgrade(&scope);
        scope
    }

    pub fn access(&self) -> Result<Access<'_, T>, Error> {
        if self.lifetime.poison.get() {
            return Err(Error::new(
                QDK_POISONED,
                "owner is poisoned; only child-first release is permitted",
            ));
        }
        if self.lifetime.active.replace(true) {
            return Err(Error::new(
                QDK_ACTIVE,
                "owner is active and cannot be reentered",
            ));
        }
        Ok(Access { handle: self })
    }

    pub fn require_releasable(&self) -> Result<(), Error> {
        if self.lifetime.borrowed.get() {
            return Err(Error::new(
                QDK_BORROWED,
                "a borrowed callback input cannot be released",
            ));
        }
        if self.lifetime.active.get() {
            return Err(Error::new(
                QDK_ACTIVE,
                "owner is active and cannot be released",
            ));
        }
        if self.lifetime.dependents.get() != 0 {
            return Err(Error::new(
                QDK_IN_USE,
                "release dependent handles before their owner",
            ));
        }
        if self.lifetime.unfinished.get() && !self.lifetime.poison.get() {
            return Err(Error::new(
                QDK_UNFINISHED,
                "finish or cancel the builder before releasing it",
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn inspect_inactive(&self) -> &T {
        assert!(!self.lifetime.active.get());
        // SAFETY: this test-only read occurs after the only active guard drops.
        unsafe { &*self.value.get() }
    }
}

impl<T> Drop for Handle<T> {
    fn drop(&mut self) {
        if let Some(address) = self.callback_address.get_mut() {
            address.set(ptr::null_mut());
        }
        #[cfg(test)]
        LIVE_HANDLES.set(LIVE_HANDLES.get() - 1);
    }
}

pub(crate) struct Access<'a, T> {
    handle: &'a Handle<T>,
}

impl<T> Deref for Access<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: access set the active flag before creating this sole guard.
        unsafe { &*self.handle.value.get() }
    }
}

impl<T> DerefMut for Access<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the active flag excludes another guard; mutable guard access
        // prevents any reference from this guard overlapping this borrow.
        unsafe { &mut *self.handle.value.get() }
    }
}

impl<T> Drop for Access<'_, T> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.handle.lifetime.poison.set(true);
        }
        self.handle.lifetime.active.set(false);
    }
}
