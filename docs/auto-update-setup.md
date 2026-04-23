# Auto-update setup

> One-time setup. Do this once as the project maintainer, then every
> future release is auto-distributed via the in-app "Check for updates"
> button in the About tab.

## How it works

Tauri's updater plugin checks a JSON endpoint
(`https://github.com/luanluciano93/madi-stack/releases/latest/download/latest.json`)
for a newer version, compares signatures against a public key baked
into the app, and if valid downloads + runs the NSIS installer in
passive mode.

Three pieces must line up:

1. A **public key** embedded in `src-tauri/tauri.conf.json` (shipped
   with every binary).
2. A **private key** stored as a GitHub Actions secret (signs every
   release asset).
3. The `tauri-action` step in `release.yml` with
   `includeUpdaterJson: true` and both secrets in `env:`.

Without the keys configured, the app won't reject updates — it just
fails to verify them and declines to install. Safe default.

## Generate the keypair (once, locally)

```bash
# Install the tauri CLI if you haven't:
cargo install tauri-cli --version "^2.0" --locked

# Produce the keypair — pick a strong password, it goes into Actions secrets.
cargo tauri signer generate -w ~/.madistack-updater.pem
```

The command outputs two files:

- `~/.madistack-updater.pem` — **private key, never commit**.
- `~/.madistack-updater.pem.pub` — public key, safe to commit.

And prints the **public key contents** to stdout — copy that.

## Embed the public key

Edit `src-tauri/tauri.conf.json`, replace the `UPDATER_PUBKEY_PLACEHOLDER`
string under `plugins.updater.pubkey` with the public key you just
generated. It looks like:

```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk...
```

Commit + push.

## Add the private key to GitHub Actions

1. Go to <https://github.com/luanluciano93/madi-stack/settings/secrets/actions>.
2. Create **two new repository secrets**:

   | Name                                | Value                                                   |
   | ----------------------------------- | ------------------------------------------------------- |
   | `TAURI_SIGNING_PRIVATE_KEY`         | Full contents of `~/.madistack-updater.pem`             |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`| The password you chose during `tauri signer generate`   |

Once both are set, the next tag push (`git push origin v0.2.0`) will
trigger `release.yml`, which builds a signed NSIS, generates
`latest.json` next to the installer, and uploads both as release assets.

## Verify it worked

After publishing the first post-signing release, open the draft release
page on GitHub. You should see, alongside the usual `-setup.exe` and
`.msi`:

- `latest.json` (small, ~1 KB)
- `*.sig` file for each installer

If any of those are missing, the signing step didn't run — double-check
the secret names.

## Rotating the key

Never. The public key baked into v0.2.0's `tauri.conf.json` is what
validates updates for v0.2.0 users forever. If you rotate, every
existing user needs to manually re-download the next version — the
in-app updater can't upgrade them because their app trusts the old key.

The only safe rotation is a hard-cut major-version release with a
manual migration path (new installer URL, comms to users). Treat the
private key like an SSL cert.

## Backup

Encrypt `~/.madistack-updater.pem` with an extra layer and stash it
somewhere durable (1Password, Bitwarden, your personal encrypted
backup). Losing the private key with no backup = same result as
rotation (orphaned user base on the last signed version).
