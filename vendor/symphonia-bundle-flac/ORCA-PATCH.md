# Local FLAC decoder backport

Based on Symphonia 0.5.5 (The Project Symphonia Developers), MPL-2.0.
Source: https://github.com/pdeljanov/Symphonia/tree/v0.5.5/symphonia-bundle-flac

33-bit side-channel support is backported from Jordan Bayles' upstream work:
https://github.com/pdeljanov/Symphonia/pull/587
Commit: 3158f50a88ae4a53478a4b393054bd944aed785b
This is a local backport of a draft upstream change, not an upstream release.

Orca adaptations: keep the 0.5 Decoder/Packet/AudioBuffer API; accept the
32-bit frame sample-size code (7); reject wasted-bit counts and predictor orders
that exceed the subframe; use wide intermediates for mid/side reconstruction;
reset packet sequence tracking after seeking at an exact frame boundary.
Only 32-bit correlated stereo allocates the additional i64 side buffer.

The bundled source files remain MPL-2.0, including these modifications.
Their notices and LICENSE must accompany redistributions. Orca's own code
retains its existing license. Tests cover subframes and complete generated
16/24/32-bit FLACs through integer and Rodio shared decoding and seeking.
