# TURN server for chatmail relays

This is a TURN server for chatmail relays
based on [webrtc.rs](https://webrtc.rs/) stack.

It uses authentication scheme described in
so-called [REST authentication](https://datatracker.ietf.org/doc/html/draft-uberti-behave-turn-rest-00).

The goal is to have a TURN server for use with
[chatmail relays](https://github.com/chatmail/relay)
distributed as a single binary.

[coturn](https://github.com/coturn/coturn/)
was considered as an alternative,
but the version distributed in Debian 12 (Bookworm)
turned out not to work with Firefox
and Ubuntu Touch webview at the time.
Debian 13 (Trixie) worked, but
chatmail relays were based on Debian 12 at the time.
