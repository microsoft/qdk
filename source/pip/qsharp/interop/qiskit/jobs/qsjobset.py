# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from concurrent.futures import Executor, Future
import datetime
from time import monotonic
import logging
from typing import Dict, List, Optional, Any
from uuid import uuid4


from qiskit.circuit import QuantumCircuit
from qiskit.providers import JobV1 as Job
from qiskit.providers import BackendV2, JobStatus, JobError
from qiskit.result.result import Result, ExperimentResult


from .qsjob import QsSimJob, RunInputCallable
from ..execution import DetaultExecutor
from .... import telemetry_events

logger = logging.getLogger(__name__)


class QsJobSet(Job):

    def __init__(
        self,
        backend: Optional[BackendV2],
        job_id: str,
        job_callable: RunInputCallable,
        run_input: List[QuantumCircuit],
        input_params: Dict[str, Any],
        executor=None,
        **kwargs,
    ) -> None:
        super().__init__(backend, job_id, **kwargs)

        self._run_input: List[QuantumCircuit] = run_input
        self._input_params: Dict[str, Any] = input_params
        self._jobs: List[QsSimJob] = []
        self._job_indexes: List[int] = []
        self._executor: Executor = executor or DetaultExecutor()
        self._job_callable = job_callable
        self._start_time: Optional[float] = None
        self._end_time: Optional[float] = None

    def submit(self):
        """Submit the job to the backend for execution.

        Raises:
            JobError: if trying to re-submit the job.
        """
        if len(self._jobs) > 0:
            raise JobError("Jobs have already been submitted.")
        self._start_time = monotonic()
        shots = self._input_params.get("shots", -1)
        telemetry_events.on_qiskit_run(shots, len(self._run_input))
        job_index = 0
        for circuit in self._run_input:
            job_id = str(uuid4())
            job = QsSimJob(
                self._backend,
                job_id,
                self._job_callable,
                [circuit],
                self._input_params,
                self._executor,
            )
            self._job_indexes.append(job_index)
            job.submit()
            job.add_done_callback(self._job_done)

            self._jobs.append(job)

    def _job_done(self, _future: Future):
        self._end_time = monotonic()
        if all(job.in_final_state() for job in self._jobs):
            # all jobs are done, so we can log the telemetry event
            shots = self._input_params.get("shots", -1)
            start_time = self._start_time or self._end_time
            duration_in_ms = (self._end_time - start_time) * 1000
            num_circuits = len(self._run_input)
            telemetry_events.on_qiskit_run_end(shots, num_circuits, duration_in_ms)

    def cancel(self):
        """Attempt to cancel the job."""
        for future in self._jobs:
            future.cancel()

    def status(self) -> JobStatus:
        """Return the status of the job, among the values of ``JobStatus``."""
        if all(job.in_final_state() for job in self._jobs):
            if any(job.status() == JobStatus.ERROR for job in self._jobs):
                return JobStatus.ERROR
            elif any(job.status() == JobStatus.CANCELLED for job in self._jobs):
                return JobStatus.CANCELLED
            assert all(job.status() == JobStatus.DONE for job in self._jobs)
            return JobStatus.DONE
        else:
            if any(job.status() == JobStatus.RUNNING for job in self._jobs):
                return JobStatus.RUNNING
            if any(job.status() == JobStatus.QUEUED for job in self._jobs):
                return JobStatus.QUEUED
            return JobStatus.INITIALIZING

    def result(self, timeout: Optional[float] = None) -> Result:
        results: List[Result] = []
        for job in self._jobs:
            results.append(job.result(timeout=timeout))

        if len(results) == 1:
            return results[0]

        output = results[0].to_dict()

        output["job_id"] = self.job_id()
        output["date"] = str(datetime.datetime.now().isoformat())
        output["backend_name"] = self.backend().name
        output["backend_version"] = self.backend().backend_version

        end_time = self._end_time or monotonic()
        start_time = self._start_time or end_time
        duration = end_time - start_time
        output["time_taken"] = str(duration)
        output["header"] = {
            "metadata": {},
        }
        output["qobj_id"] = str(uuid4())
        output["success"] = all(result.success for result in results)
        agg_result: List[ExperimentResult] = []
        for result in results:
            for experiment_result in result.results:
                agg_result.append(experiment_result.to_dict())
        output["results"] = agg_result
        output = Result.from_dict(output)
        return output
