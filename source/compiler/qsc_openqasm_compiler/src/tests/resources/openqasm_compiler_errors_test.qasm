// If there is a squiggle at the beginning of the file, it means some of the spans is incorrect.

include "stdgates.inc";

// Not an error. Utility qubits to be used in the rest of the file.
qubit q;
qubit[1] qreg_1;
qubit[2] qreg_2;

// NotSupported defcalgrammar
defcalgrammar "openpulse";

// NotSupported cal
cal {
   // Defined within `cal`, so it may not leak back out to the enclosing blocks scope
   float new_freq = 5.2e9;
   // declare global port
   extern port d0;
   // reference `freq` variable from enclosing blocks scope
   frame d0f = newframe(d0, freq, 0.0);
}

// NotSupported defcal
defcal x $0 {
   waveform xp = gaussian(1.0, 160t, 40dt);
   // References frame and `new_freq` declared in top-level cal block
   play(d0f, xp);
   set_frequency(d0f, new_freq);
   play(d0f, xp);
}

// NotSupported
delay [2ns] q;

box [2ns] { // NotSupported box duration
    x [2ns] q; // NotSupported duration on gate call
}

for int i in [0:2] {
    break; // NotSupported break
}

for int i in [0:2] {
    continue; // NotSupported continue
}


// NotSupported mutable array reference
def mut_subroutine_dyn(mutable array[int[8], #dim = 1] arr_arg) {

}

// NotSupported mutable static sized array reference
def mut_subroutine_static(mutable array[int[8], 2, 3] arr_arg) {

}

// currently blocked by type checker
// unimplemented
//bit[2] creg1;
//bit[10] creg2;
//let concatenated_creg = creg1 ++ creg2;

// currently blocked by type checker
// unimplemented
//qubit[2] one;
//qubit[10] two;
//let concatenated = one ++ two;


//duration dur0 = 2ns;
//duration dur1 = 3ns;

// NotSupported stretch types values
//stretch stretch_val = dur0 - dur1;

// Unimplemented
extern extern_func(int);

// NotSupported hardware qubit
x $0;
