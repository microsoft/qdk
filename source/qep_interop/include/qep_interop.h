#ifndef RUSTLIB_H
#define RUSTLIB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initializes backend (see README for possible values).
   Must be called before calling any quantum instructions.
   If called again, old backend will be discarded and replaced with new one. */
void qdk_init(const char *backend);

/* Saves profiler output to JSON at given path. 
   Does nothing if backend is not profiler. */
void qdk_save_profile(const char *path);

/* Quantum instructions not returning result. Rotation angles are in radians. */
void qdk_ccx(size_t control0, size_t control1, size_t target);
void qdk_cx(size_t control, size_t target);
void qdk_cy(size_t control, size_t target);
void qdk_cz(size_t control, size_t target);
void qdk_rx(size_t qubit_id, double angle);
void qdk_rxx(size_t qubit0, size_t qubit1, double angle);
void qdk_ry(size_t qubit_id, double angle);
void qdk_ryy(size_t qubit0, size_t qubit1, double angle);
void qdk_rz(size_t qubit_id, double angle);
void qdk_rzz(size_t qubit0, size_t qubit1, double angle);
void qdk_h(size_t qubit_id);
void qdk_s(size_t qubit_id);
void qdk_sadj(size_t qubit_id);
void qdk_sx(size_t qubit_id);
void qdk_t(size_t qubit_id);
void qdk_tadj(size_t qubit_id);
void qdk_x(size_t qubit_id);
void qdk_y(size_t qubit_id);
void qdk_z(size_t qubit_id);
void qdk_swap(size_t qubit0, size_t qubit1);
void qdk_reset(size_t qubit_id);
void qdk_mz_store(size_t qubit_id, size_t result_id);
void qdk_mz_store_reset(size_t qubit_id, size_t result_id);

/* Quantum instructions returning a result. */
size_t qdk_read_result(size_t result_id);
size_t qdk_mz(size_t qubit_id);
size_t qdk_mz_reset(size_t qubit_id);

#ifdef __cplusplus
}
#endif

#endif