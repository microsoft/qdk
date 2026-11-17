#include <stdio.h>
#include "qspnlib.h"


/// Reads integer register, then resets it.
int read_le_int_register(int start_qubit_id, int length) {
    int ans = 0;
    for(int i=0;i<length; i++) {
        ans+= qsp_mz_reset(start_qubit_id+i)<<i;
    }
    return ans;
}

/// Generates GHZ state on n qubits and measures it.
/// Returns measurement result as little-endian integer. Resets all qubits.
int ghz_example(int n) {
    qsp_h(0);
    for(int i=1;i<n;i++) {
        qsp_cx(0, i);
    }
    return read_le_int_register(0, n);
}


int main(void) {
    qsp_init("sparse_sim");
    for(int shot=0;shot<5;shot++) {
        int result = ghz_example(10);
        printf("result = %d\n", result);
    }

    qsp_init("temporal_profiler_with_trace");
    ghz_example(10);
    qsp_save_profile("source/profiling/profile_viewer/profiles/ghz_profile.json");

    return 0;
}
