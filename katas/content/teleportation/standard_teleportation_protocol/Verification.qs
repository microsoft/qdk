namespace Kata.Verification {
    open Microsoft.Quantum.Diagnostics;
    open Microsoft.Quantum.Katas;

    @EntryPoint()
    operation CheckSolution() : Bool {        
        return CheckTeleportationCompleteWithFeedback(Kata.StandardTeleport);
    }

}
