# Credit

Ali Shahin Shamsabadi, Brian R. Bondy, and Brendan Eich developed the idea behind this
project: that indirect prompt injection can be made structurally impossible rather than merely
unlikely, by enforcing information-flow labels at every boundary and separating routing from
content so untrusted text cannot redirect an action.

The mobile work is by [chrislacy](https://github.com/chrislacy). The desktop work is by
[Aubrey Keus](https://github.com/aekeus) and [Agustín Ruiz](https://github.com/aguscruiz). The
design work is by Agustín Ruiz. The formal verification proofs are by Anthony Tseng
([darkdh](https://github.com/darkdh)).

Security help and guidance came from Yan Zhu ([diracdeltas](https://github.com/diracdeltas)),
Andrea Brancaleoni ([thypon](https://github.com/thypon)) and Paolo Perego
([thesp0nge](https://github.com/thesp0nge)), and from the HackerOne reporters who reported
issues.

Everyone who has worked on it is listed under
[contributors](https://github.com/brave/bravebot/graphs/contributors).

[SafeHouse](https://github.com/brave-experiments/safehouse) is the research driver and proof of
concept behind the product, bravebot, this repository.

The model backend for the Brave gateway is [brave/aichat](https://github.com/brave/ai-gateway). The client-side handling
it builds on comes from [brave/brave-core](https://github.com/brave/brave-core). The dockerized
reproducible build setup is from [bbondy/guardrails](https://github.com/bbondy/guardrails), as are
the classifier prompt and the two-block marker strategy behind
[vetting](specs/vetting.md).
