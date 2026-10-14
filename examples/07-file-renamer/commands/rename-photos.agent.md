# Import: ../functions/photo-stamp.agent.md

> Renames camera photos like IMG_20261003_141500.jpg to 2026-10-03_photo-1.jpg. Other files are left alone.
> With dry_run it only prints what would change.
# Command: rename-photos
- args: folder: text, dry_run: bool = false

## Run
- file list folder -> names
- set renamed = 0
- for each name in names:
  - set stamp = photo_stamp(name)
  - if stamp is not empty:
    - set renamed = renamed + 1
    - set target = "{stamp}_photo-{renamed}.{path.extension(name)}"
    - print: "{name} -> {target}"
    - if not dry_run:
      - file rename path.join(folder, name) to path.join(folder, target)
  - else:
    - print: "skipped {name}"
- if dry_run:
  - print: "Dry run: {renamed} files would be renamed"
- else:
  - print: "Renamed {renamed} files"
