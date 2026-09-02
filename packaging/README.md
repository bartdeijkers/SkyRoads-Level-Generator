# Package documentation sources

`README-WINDOWS.txt` and `README-LINUX.txt` are the canonical platform notes
for generated release archives. Package assembly must copy the matching file
without renaming it and must also include the repository `README.md`.

Before publishing an archive, update its platform note with the acceptance
evidence gathered for that exact build: operating-system version, SDL runtime,
connection mode, controller firmware, and mapped/raw result. The current SN30
Pro Bluetooth evidence is partial and does not complete an acceptance row. Do
not replace that boundary with a support claim until the controller hardware
matrix in [`plans/expand-controller-support.md`](../plans/expand-controller-support.md)
has passed.

Do not add a controller mapping file speculatively. Include one only after a
physical-device failure proves that the packaged SDL mappings are insufficient,
and record the mapping source and license alongside it.
