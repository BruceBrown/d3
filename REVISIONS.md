# d3 -- Revision History

## Reorganized d3-lib into d3-core
Now that things are a bit more stable, `d3-lib`, which is built as a set of libraries, needs some help. Its a bit pointless to have the netsted library structure, as you need it all for things to work. So, things that were previously libraries under `d3-lib` are now just folders under `d3-core/src`. That rippled some changes upward,
however the code remains the same, just the names and locations moved.

As part of the change `d3-derive` was pulled up as a sibling of `d3-core`. If Rust ever allows macros in regular libraries, then it may move back under core. The test instruction set was also moved up as a sibling, as it is dependent upon the `#[derive(MachineImpl)]` and the underlying support in `d3-core`. It is now named `d3-dev-instruction-sets`.

Finally, I worked on the executor a bit more. Mostly a code cleanup and refactoring. Executror state and idle time is now exposed. I was hoping it would allow for better decision making on spawnnig threads when the current set of executors are blocked on send and they exhaust their backoff period. Its not an ideal solution, however if additional threads aren't spawned the entire system risks a lockup as receivers, which drain queus aren't able to run due to writes blocking all the executors.

Next on the docket is to cleanup integrated tests, which haven't kept up with recent code changes. Also, need to add UDP support.

## Forwarder Multiplier and Executor Fixes
First revision... added this file.

Added multiplier to Forwarder. This allows you to tell the Forwarder how many messages it should forward for every message it receives. This is a geometric hit to the executor and exposed a problem that I was having a difficult time reproducing: a sender that keeps filling the  message queue without returning back to the receive call. This potentially blocks an executor. For example, having a daisy-chain of 10 machines, with each machine having a multiplier of 5, will have that last notifier receiving 1,953,125 messages.

While spinning up new executors isn't a good thing, having a system grind to a halt is worse. So, we'll spin up a new executor and let the blocked one die after it completes its send. This also shows how important sizing is. Add a larger queue depth, and the problem goes away. Add more executors, and it goes away as well.

To be added: Find a way of determing if some executors are idle when blocking occurs, and if so, don't spin up a new executor as it will be of no value.
