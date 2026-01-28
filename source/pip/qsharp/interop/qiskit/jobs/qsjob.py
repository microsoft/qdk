# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from abc import ABC, abstractmethod
from concurrent.futures import Executor, Future
import logging
from time import monotonic
from typing import Callable, Dict, Optional, Any

from qiskit.providers import BackendV2
from qiskit.circuit import (
    QuantumCircuit,
)

from qiskit.result import Result
from qiskit.providers import JobV1, JobStatus, JobError

from ..execution import DetaultExecutor
from .... import telemetry_events
from ....estimator import EstimatorResult

logger = logging.getLogger(__name__)

RunInputCallable = Callable[[QuantumCircuit, str, Dict[str, Any]], Result]


class QsJob(JobV1, ABC):

    def __init__(
        self,
        backend: Optional[BackendV2],
        job_id: str,
        job_callable: RunInputCallable,
        run_input: QuantumCircuit,
        input_params: Dict[str, Any],
        executor=None,
        **kwargs,
    ) -> None:
        super().__init__(backend, job_id, **kwargs)

        self._run_input = run_input
        self._input_params = input_params
        self._future: Optional[Future] = None
        self._executor: Executor = executor or DetaultExecutor()
        self._job_callable = job_callable
        self._status = JobStatus.INITIALIZING
        self._submit_start_time: Optional[float] = None

    def submit(self):
        """Submit the job to the backend for execution.

        Raises:
            JobError: if trying to re-submit the job.
        """
        if self._future is not None:
            raise JobError("Job has already been submitted.")

        self._submit_start_time = monotonic()
        self._future = self._executor.submit(
            self._job_callable, self._run_input, self.job_id(), **self._input_params
        )
        self.add_done_callback(self._submit_duration)

    @abstractmethod
    def result(self, timeout: Optional[float] = None) -> Any:
        pass

    @abstractmethod
    def _submit_duration(self, _future: Future):
        pass

    def _result(self, timeout: Optional[float] = None) -> Any:
        """Return the results of the job."""
        if self._future is None:
            raise JobError("Job has not been submitted.")

        return self._future.result(timeout=timeout)

    def status(self) -> JobStatus:
        """Return the status of the job, among the values of ``JobStatus``."""
        if self._future is None:
            return JobStatus.INITIALIZING
        if self._future.cancelled():
            return JobStatus.CANCELLED
        if self._future.done():
            if self._future.exception() is None:
                return JobStatus.DONE
            else:
                return JobStatus.ERROR
        if self._future.running():
            return JobStatus.RUNNING

        return JobStatus.INITIALIZING

    def backend(self) -> BackendV2:
        """Return the backend where this job was executed."""

        return super().backend()

    def cancel(self):
        """Attempt to cancel the job."""
        if self._future is not None:
            self._future.cancel()

    def error(self) -> Optional[JobError]:
        """Return the error that occurred during the execution of the job."""
        if self._future is not None:
            return self._future.exception()
        return None

    def add_done_callback(self, fn: Callable[[Future[Result]], object]) -> None:
        """Attaches a callable that will be called when the job finishes."""
        if self._future is not None:
            self._future.add_done_callback(fn)


class QsSimJob(QsJob):

    def result(self, timeout: Optional[float] = None) -> Result:
        return self._result(timeout=timeout)

    def submit(self):
        """Submit the job to the backend for execution.

        Raises:
            JobError: if trying to re-submit the job.
        """
        shots = self._input_params.get("shots", -1)
        telemetry_events.on_qiskit_run(shots, 1)

        super().submit()

    def _submit_duration(self, _future: Future):
        end_time = monotonic()
        # _submit_start_time is set in submit() before adding this callback
        assert self._submit_start_time is not None
        duration_in_sec = end_time - self._submit_start_time
        duration_in_ms = duration_in_sec * 1000

        shots = self._input_params.get("shots", -1)
        telemetry_events.on_qiskit_run_end(shots, 1, duration_in_ms)


class ReJob(QsJob):

    def result(self, timeout: Optional[float] = None) -> EstimatorResult:
        return self._result(timeout=timeout)

    def submit(self):
        """Submit the job to the backend for execution.

        Raises:
            JobError: if trying to re-submit the job.
        """

        telemetry_events.on_qiskit_run_re()

        super().submit()

    def _submit_duration(self, _future: Future):
        end_time = monotonic()
        # _submit_start_time is set in submit() before adding this callback
        assert self._submit_start_time is not None
        duration_in_sec = end_time - self._submit_start_time
        duration_in_ms = duration_in_sec * 1000

        telemetry_events.on_qiskit_run_re_end(duration_in_ms)
