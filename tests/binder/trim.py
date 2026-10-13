#!/usr/bin/env python3
"""Trim a rustdoc JSON file to what binder/rust reads (see src/binder/rust/rustdoc.rs).

usage: trim.py <full.json> <trimmed.json>

Keeps only the items the binder's walk can reach (public modules, `pub use` targets, impls of
public types minus blanket/synthetic/negative impls and std-plumbing traits), strips spans,
links, attributes and example code from docs, and writes minified JSON.
"""
import json
import sys

NOISE = set("""Clone Copy Debug Display Default PartialEq Eq PartialOrd Ord Hash From Into TryFrom TryInto
Drop Deref DerefMut AsRef AsMut Borrow BorrowMut ToOwned ToString Send Sync Unpin UnwindSafe
RefUnwindSafe Any Add Sub Mul Div Rem Neg Not AddAssign SubAssign MulAssign DivAssign RemAssign
BitAnd BitOr BitXor Shl Shr FromStr Iterator IntoIterator DoubleEndedIterator ExactSizeIterator
Extend FromIterator Error Future IntoFuture Sum Product Index IndexMut StructuralPartialEq
CloneToUninit Pointer LowerHex UpperHex Octal Binary LowerExp UpperExp Write Read BufRead Seek
Serialize Deserialize Arbitrary Step Fn FnMut FnOnce""".split())


def kind(it):
    return next(iter(it["inner"])) if it.get("inner") else ""


def is_public(it):
    return it.get("visibility") == "public" and not any("doc(hidden)" in a for a in it.get("attrs", []))


def short_docs(d):
    if not d:
        return d
    d = d.split("```")[0]
    paras = [p for p in d.split("\n\n") if p.strip()]
    return "\n\n".join(paras[:2])[:600]


def main(src, dst):
    doc = json.load(open(src))
    index = doc["index"]
    get = lambda i: index.get(str(i))
    keep = {}
    seen_modules = set()

    def keep_item(i, it):
        keep[i] = it

    def walk_module(mid):
        m = get(mid)
        keep_item(mid, m)
        for cid in m["inner"]["module"]["items"]:
            visit(cid)

    def visit(i, alias=False):
        it = get(i)
        if it is None:
            return
        k = kind(it)
        if k == "use":
            u = it["inner"]["use"]
            keep_item(i, it)
            t = u.get("id")
            if t is not None and get(t) is not None:
                if u.get("is_glob"):
                    if t not in seen_modules:
                        seen_modules.add(t)
                        walk_module(t)
                elif kind(get(t)) == "module":
                    if t not in seen_modules:
                        seen_modules.add(t)
                        walk_module(t)
                else:
                    visit(t, alias=True)
                if t not in keep:
                    keep_item(t, get(t))  # stub: the reader only asks "is the target local?"
            return
        if not is_public(it) and not alias:
            return
        if k == "module":
            if i not in seen_modules:
                seen_modules.add(i)
                walk_module(i)
        elif k in ("function", "trait", "type_alias"):
            keep_item(i, it)
            if k == "type_alias":
                t = it["inner"]["type_alias"]["type"].get("resolved_path", {}).get("id")
                if t is not None and get(t) is not None and t not in keep:
                    keep_item(t, get(t))  # stub kept so "is the target local?" stays true
        elif k in ("struct", "enum", "union"):
            keep_item(i, it)
            for iid in it["inner"][k]["impls"]:
                imp = get(iid)
                if imp is None:
                    continue
                inner = imp["inner"]["impl"]
                if inner["is_synthetic"] or inner["blanket_impl"] is not None or inner["is_negative"]:
                    continue
                tr = inner["trait"]
                if tr is not None and tr["path"].split("::")[-1] in NOISE:
                    continue
                keep_item(iid, imp)
                for fid in inner["items"]:
                    f = get(fid)
                    if f is not None and kind(f) == "function":
                        keep_item(fid, f)

    root = doc["root"]
    seen_modules.add(root)
    walk_module(root)

    out_index = {}
    for i, it in keep.items():
        k = kind(it)
        body = it["inner"][k]
        o = {"id": it["id"], "name": it.get("name"), "visibility": it.get("visibility"),
             "docs": short_docs(it.get("docs")), "inner": {}}
        hidden = [a for a in it.get("attrs", []) if "doc(hidden)" in a]
        if hidden:
            o["attrs"] = hidden
        if k == "module":
            o["inner"]["module"] = {"items": body["items"]}
        elif k == "use":
            o["inner"]["use"] = {key: body.get(key) for key in ("source", "name", "id", "is_glob")}
        elif k in ("struct", "enum", "union"):
            o["inner"][k] = {"impls": body["impls"]}
        elif k == "impl":
            o["inner"]["impl"] = {key: body.get(key) for key in
                                  ("is_synthetic", "blanket_impl", "is_negative", "trait", "for", "items", "generics")}
        elif k == "function":
            o["inner"]["function"] = {key: body.get(key) for key in ("sig", "generics", "header")}
        elif k == "type_alias":
            o["inner"]["type_alias"] = {"type": body["type"]}
        else:
            o["inner"][k] = {}  # stub (trait, constant, ...): only existence and kind matter
        out_index[str(i)] = o

    ids = set()

    def collect(x):
        if isinstance(x, dict):
            if isinstance(x.get("id"), int) and "path" in x:
                ids.add(x["id"])
            for v in x.values():
                collect(v)
        elif isinstance(x, list):
            for v in x:
                collect(v)

    for o in out_index.values():
        collect(o["inner"])
    paths = {str(i): {"path": doc["paths"][str(i)]["path"]} for i in ids if str(i) in doc["paths"]}
    res = {"format_version": doc["format_version"], "crate_version": doc["crate_version"],
           "root": root, "index": out_index, "paths": paths}
    with open(dst, "w") as f:
        json.dump(res, f, separators=(",", ":"))
    print(f"{dst}: {len(out_index)} items, {len(json.dumps(res, separators=(',', ':'))) // 1024} KiB")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
