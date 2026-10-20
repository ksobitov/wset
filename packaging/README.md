# Packaging

Pushing a `v*` tag runs [release.yml](../.github/workflows/release.yml).
It runs the tests, builds static amd64 and arm64 .debs with cargo-deb,
attaches them to a GitHub release, and pushes a signed APT repository, made
by [apt/build-repo.sh](apt/build-repo.sh), to the `gh-pages` branch.
The repository keeps the packages of earlier releases, and only the newest
tag updates it. A failed run can be re-run; it replaces the release's
packages.

One-time setup, the same as for wbanish:

1. Add the APT signing key as the `APT_SIGNING_KEY` secret under Settings →
   Secrets and variables → Actions; wbanish's key can be reused:

   ```sh
   gpg --armor --export-secret-keys "wbanish APT repository" | clip.exe
   ```

2. After the first release has run, set Settings → Pages → Source to
   "Deploy from a branch", `gh-pages`, `/ (root)`.

To release, bump the version in Cargo.toml, commit, then:

```sh
git tag v0.1.0
git push origin v0.1.0
```
