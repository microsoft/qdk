// # Sample
// Fail Statement
//
// # Description
// A fail statement collects information about the current state of the program,
// and then terminates execution entirely. The collected information will be
// presented to the user along with the message specified with the fail statement.

operation Main() : Unit {
    use q = Qubit();
    X(q);
    if M(q) == Zero {
        // Fail statements are useful when representing control flow
        // for unreachable lines.
        fail "Measurement should have been `One`, how did we get here?"
    }
    Reset(q);

    let computerIsOnFire = false;
    // Fail statements are also useful for bailing out of unrecoverable states
    if computerIsOnFire {
        fail "The computer is on fire, terminating execution."
    }
}
