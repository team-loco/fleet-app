# Fleet

Native multi-cluster Kubernetes desktop app with Flux built in. See `docs/PLAN.md`.

On macOS, `mise run bundle` builds a release, ad-hoc signed `dist/Fleet.app`;
`mise run bundle:loco` includes Loco support. `fleet --version` prints the workspace version and git revision.

`mise run gen:icon` regenerates `apps/fleet/resources/Fleet.icns` and `Fleet.png` from
`assets/fleet-logo.png` using the macOS Command Line Tools (swift, sips, iconutil).
