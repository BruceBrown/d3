# d3 -- Revision History

## Forwarder multiplier and executor fixes
First revision... added this file.

Added multiplier to Forwarder. This allows you to tell the Forwarder how many messages it should forward for every message it receives. This is a geometric hit to the executor and exposed a problem that I was having a difficult time reproducing: a sender that keeps filling the  message queue without returning back to the receive call. This potentially blocks an executor. For example, having a daisy-chain of 10 machines, with each machine having a multiplier of 5, will have that last notifier receiving 1,953,125 messages.

While spinning up new executors isn't a good thing, having a system grind to a halt is worse. So, we'll spin up a new executor and let the blocked one die after it completes its send. This also shows how important sizing is. Add a larger queue depth, and the problem goes away. Add more executors, and it goes away as well.

To be added: Find a way of determing if some executors are idle when blocking occurs, and if so, don't spin up a new executor as it will be of no value.
