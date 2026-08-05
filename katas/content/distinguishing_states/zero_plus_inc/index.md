**Input:** A qubit which is guaranteed to be in either the $\ket{0}$ or the $\ket{+}$ state.
    
**Output:** 
* 0 if the qubit was in the $\ket{0}$ state, 
* 1 if it was in the $\ket{+}$ state,
* -1 if you can't decide, i.e., an "inconclusive" result. 

Your solution:

* should never give 0 or 1 answer incorrectly (i.e., identify $\ket{0}$ as 1 or $\ket{+}$ as 0),
* will be called multiple times, with one of the states picked with equal probability every time,
* may give an inconclusive (-1) answer in at most 80% of all the cases,
* must correctly identify the $\ket{0}$ state as 0 in at least 10% of all the cases,
* must correctly identify the $\ket{+}$ state as 1 in at least 10% of all the cases.
    
The state of the qubit at the end of the operation does not matter.
    
> This task is an example of unambiguous state discrimination.
