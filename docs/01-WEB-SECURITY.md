# 01-WEB-SECURITY.md — hostile network, hostile names, hostile apps

### Status: STUB. The design is complete in plan-webnext.md §10 (8-round
### adversarial review, SHIP). This file is the P0 extraction target; the
### normative adversary matrix + test vectors are P0 work. Enforces
### Constitution rules 1, 3, 4, 7, 8.

## The rule everything rests on
> Discovery can be messy. Execution cannot. (plan-webnext.md §10)

## Adversary matrix (the auditable core — expand to full test vectors in P0)

| Adversary | Attack | Required defense |
|---|---|---|
| Gateway / CDN | serves wrong / old / partial bytes | `b3:` verification hard-stop; no "continue anyway" |
| Publisher-key thief | signs a malicious update | threshold signing + update delay + revocation (§10.5) |
| Search engine | ranks a clone above the real app | search is a pointer, never grants trust |
| Bare-name squatter | `web://bank` phishing | C4 chooser + high-risk (R7) conservative mode |
| Malicious app | asks for broad powers | manifest caps + trusted prompts + runs-and-observes |
| Malicious update | adds RPC / identity / payment | U-ladder diff pauses install (U3+) |
| Broker | fakes WVEP receipts | accepted-broker signatures + reputation + audit log |
| Compute worker | DDoS / cryptojack | no ambient network (broker-only) + hard budgets |
| Storage task | illegal / private content | encrypted/public-only policy + quotas |
| Judgment broker | abusive labeling tasks | category controls + broker accountability + user policy |
| Local malware | steals master key | OS keychain / TPM + 3-tier recovery (§10.5) |
| Government censor | blocks gateways / logs | `b3:`/`ed:` offline / cache / mirror / USB path |

## MUST NEVER (the invariant list)
```
execute unverified bytes            · continue after a hash mismatch
trust a bare name                   · let an app draw browser security chrome
let an app run compute directly     · let a worker use arbitrary network
allow silent permission expansion   · auto-accept key rotation
expose a global user identity       · treat broker/site/search/log as trusted
```

## Certainty ladder (C0–C5) — see plan-webnext.md §10.3 / §18
Dangerous capabilities (payments, identity-sharing, private profiles, WVEP V3+)
require **C1/C2 or better**, never C4/C5.

*(Full seven-rule model, C0–C5 chrome treatment, the 12-adversary sweep with
verdicts, trusted-chrome §10.6, key recovery §10.5, and normative test vectors:
extract from plan-webnext.md §10 in P0.)*
