# retoc sidecar staging

Release builds include retoc 0.1.5 as a Tauri sidecar. The executable itself is
generated locally and ignored by Git. Run `npm run prepare:retoc`, or set
`RETOC_SOURCE` to a trusted local retoc 0.1.5 executable before running the
script. The script accepts only the official upstream GitHub release, verifies
the upstream checksum, and writes the target-qualified sidecar filename Tauri
expects.
