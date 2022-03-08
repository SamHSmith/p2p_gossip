# p2p_gossip

This is a simple peer-to-peer gossiping application in Rust. What does that entail? Each instance of the program acts as a "peer", every N seconds it sends a random gossip message to all the peers it is connected to. Those peers then forward the message to all the peers they are connected to so that the end result is that all the peers have received the message. The peers discover new peers to connect to by talking to those it already knows. When peers go down or misbehave they are simply dropped, the network continues without them.

### Building and Running
To build the program simply run `cargo build` which will produce the binary in `target/debug/p2p_gossip`. If you wish to run the test suite use `cargo test` like any other rust project. If you want to see the console output from these tests use `cargo test -- --nocapture`.

```
# You start an initial peer as follows
./p2p_gossip --port=25532 --period=8
# This starts a peer on ipv4 port 25532 with a messaging period of 8 seconds.
# If you want to start it on ipv6 pass the --use-ipv6 flag.

# Subsequent peers can be started the same way. You use the --connect flag in order to make them connect to a first
# initial other peer. Here is an example where we create a network of three peers.

./p2p_gossip --port=25532 --period=8
./p2p_gossip --connect="127.0.0.1:25532" --port=25533 --period=15
./p2p_gossip --port=25534 --connect="127.0.0.1:25533" --period=2

# Here is the corresponding example for ipv6
./p2p_gossip --port=25532 --period=8 --use-ipv6
./p2p_gossip --connect="[::1]:25532" --port=25533 --period=15
./p2p_gossip --port=25534 --connect="[::1]:25533" --period=2
