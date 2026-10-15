# Uses
- [photo_stamp](../functions/photo-stamp.agent.md)

# Command: rename-photos
Renames camera photos like IMG_20261003_141500.jpg to 2026-10-03_photo-1.jpg. Other files are left alone.
With dry_run it only prints what would change.

- in: folder: text, dry_run: bool = false
- steps:
  1. file list folder into names
  2. 0 into renamed
  3. for each name in names:
     1. if photo_stamp(name) is some stamp:
        1. renamed + 1 into renamed
        2. "{stamp}_photo-{renamed}.{path.extension(name)}" into target
        3. print: "{name} -> {target}"
        4. if not dry_run:
           1. file rename path.join(folder, name) to path.join(folder, target)
        else:
        1. print: "skipped {name}"
  4. if dry_run:
     1. print: "Dry run: {renamed} files would be renamed"
     else:
     1. print: "Renamed {renamed} files"
