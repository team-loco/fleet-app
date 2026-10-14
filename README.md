# Flock

Native multi-cluster Kubernetes desktop app with Flux built in. See `docs/PLAN.md`.

On macOS, `mise run bundle` builds a release, ad-hoc signed `dist/Flock.app`;
`mise run bundle:loco` includes Loco support. `flock --version` prints the workspace version and git revision.

`mise run gen:icon` regenerates `apps/flock/resources/Flock.icns` and `Flock.png` from
`assets/flock-logo.png` using the macOS Command Line Tools (swift, sips, iconutil).
