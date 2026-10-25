#include <stdio.h>
#include "qep_interop.h"


/// Reads integer register, then resets it.
int read_le_int_register(int start_qubit_id, int length) {
    int ans = 0;
    for(int i=0;i<length; i++) {
        ans+= mz_reset(start_qubit_id+i)<<i;
    }
    return ans;
}

/// Generates GHZ state on n qubits and measures it.
/// Returns measurement result as little0endian integer. Resets all qubits.
int ghz_example(int n) {
    h(0);
    for(int i=1;i<n;i++) {
        cx(0, i);
    }
    return read_le_int_register(0, n);
}


int main(void) {
    init("sparse_sim");
    for(int shot=0;shot<5;shot++) {
        int result = ghz_example(10);
        printf("result = %d\n", result);
    }

    init("counting_profiler");
    ghz_example(10);
    save_profile("source/qep/tmp/ghz_profile.json");

    return 0;
}