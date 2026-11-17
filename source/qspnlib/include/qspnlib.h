#ifndef RUSTLIB_H
#define RUSTLIB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initializes backend (see README for possible values).
   Optional: defaults to stdio (instructions on stdout, results on stdin).
   On Unix, QDK_INSTRUCTION_FD and QDK_RESULT_FD select dedicated protocol pipes.
   If called again, old backend and pending instructions will be discarded. */
void qsp_init(const char *backend);

/* Flushes pending instructions and saves profiler output to JSON at given path.
   Does not write a file if backend is not profiler.
   In stdio mode, only prints guidance to stderr; does not flush or inspect path. */
void qsp_save_profile(const char *path);

/* Quantum instructions not returning result. Rotation angles are in radians. */
void qsp_ccx(size_t control0, size_t control1, size_t target);
void qsp_and_adj(size_t control0, size_t control1, size_t target);
void qsp_cx(size_t control, size_t target);
void qsp_cy(size_t control, size_t target);
void qsp_cz(size_t control, size_t target);
void qsp_rx(size_t qubit_id, double angle);
void qsp_rxx(size_t qubit0, size_t qubit1, double angle);
void qsp_ry(size_t qubit_id, double angle);
void qsp_ryy(size_t qubit0, size_t qubit1, double angle);
void qsp_rz(size_t qubit_id, double angle);
void qsp_rzz(size_t qubit0, size_t qubit1, double angle);
void qsp_h(size_t qubit_id);
void qsp_s(size_t qubit_id);
void qsp_sadj(size_t qubit_id);
void qsp_sx(size_t qubit_id);
void qsp_t(size_t qubit_id);
void qsp_tadj(size_t qubit_id);
void qsp_x(size_t qubit_id);
void qsp_y(size_t qubit_id);
void qsp_z(size_t qubit_id);
void qsp_swap(size_t qubit0, size_t qubit1);
void qsp_reset(size_t qubit_id);
void qsp_mz_store(size_t qubit_id, size_t result_id);
void qsp_mz_store_reset(size_t qubit_id, size_t result_id);

/* Quantum instructions returning a result. */
size_t qsp_read_result(size_t result_id);
size_t qsp_mz(size_t qubit_id);
size_t qsp_mz_reset(size_t qubit_id);

/* Reads stored result IDs 0..count-1 into output using 0/1/2 for Zero/One/Error. */
void qsp_read_results(size_t count, uint8_t *output);

#ifdef __cplusplus
}
#endif

#endif
