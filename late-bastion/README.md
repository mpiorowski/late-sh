# late-bastion

`late-bastion` terminates public SSH and holds the connection while `late-ssh`
backends are replaced. It authenticates the user's key, carries shell and
bounded CLI exec requests over the private `/tunnel` WebSocket, and preserves
the original client IP. Account, ban, TUI and pairing decisions stay in
`late-ssh`.

- [Current design and open work](../devdocs/BASTION-FEATURE.md)
- [Local qualification and remaining live checks](../devdocs/BASTION-QA.md)
- [First production cutover and rollback](../devdocs/BASTION-DEPLOY-GUIDE.md)

For a local interactive run, start the Compose stack with `make start` from
the repository root, then connect with `ssh -p 5222 localhost`. The direct
backend comparison port is `2222`. See the QA doc for repeatable test commands
and the limits of local verification.
