#!/usr/bin/env python3
"""Exhaustively compare two declared finite policies, including rejection payload."""
import argparse
import hashlib
import json
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def decode(codec):
    require(codec[:8] == b"LPCC\x01\x01\x00\x00", "header")
    offsets = [int.from_bytes(codec[8 + 2*i:10 + 2*i], "big") for i in range(257)]
    require(offsets[0] == 0 and 522 + 3*offsets[-1] == len(codec), "complete body")
    require(all(a < b for a,b in zip(offsets, offsets[1:])), "nonempty columns")
    raw = bytearray(131072)
    for g in range(256):
        records = [tuple(codec[522+3*k:525+3*k]) for k in range(offsets[g], offsets[g+1])]
        require(records[0][0] == 0, "first red")
        require(all(a[0] < b[0] and a[1:] != b[1:] for a,b in zip(records, records[1:])), "canonical runs")
        for i, (r, lo, hi) in enumerate(records):
            require(lo <= hi or (lo, hi) == (255,0), "canonical interval")
            end = records[i+1][0] if i+1 < len(records) else 256
            for red in range(r,end):
                k = 2*(256*red+g)
                raw[k:k+2] = bytes((lo,hi))
    return bytes(raw)


def encode(raw):
    require(len(raw) == 131072, "raw size")
    records = []
    offsets = [0]
    for g in range(256):
        previous = None
        for r in range(256):
            pair = tuple(raw[2*(256*r+g):2*(256*r+g)+2])
            require(pair[0] <= pair[1] or pair == (255,0), "canonical source interval")
            if pair != previous:
                records.append(bytes((r,*pair)))
                previous = pair
        offsets.append(len(records))
    require(offsets[-1] <= 65535, "offset width")
    return b"LPCC\x01\x01\x00\x00" + b"".join(x.to_bytes(2,"big") for x in offsets) + b"".join(records)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("old_codec",type=Path)
    parser.add_argument("new_raw",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    old_codec = args.old_codec.read_bytes()
    require(hashlib.sha256(old_codec).hexdigest() == "aa6aa7c0b630437f1c1ba8c2ceafb0dadf6551c42331559504076a6cd44e6331", "old codec pin")
    old = decode(old_codec)
    require(hashlib.sha256(old).hexdigest() == "97bcc9f793adb7f13bd70c89e9788c8ab61baf8c77e9f8cd80335ad767d71ae2", "old raw pin")
    new = args.new_raw.read_bytes()
    require(hashlib.sha256(new).hexdigest() == "cf42419977850c435ede3e5be05a0a74b14ad98255418aac7d396f5bb501550e", "new raw pin")
    new_codec = encode(new)
    require(decode(new_codec) == new, "codec roundtrip")
    require(encode(old) == old_codec, "historical canonical codec roundtrip")
    counts = dict(visited=0, old_accepted=0, new_accepted=0, accept_to_reject=0, reject_to_accept=0, rejection_payload_changes=0, unchanged=0)
    examples = {k:[] for k in ["accept_to_reject","reject_to_accept","rejection_payload_changes"]}
    for r in range(256):
        for g in range(256):
            k = 2*(256*r+g)
            olo,ohi = old[k:k+2]
            nlo,nhi = new[k:k+2]
            for b in range(256):
                neutral = r == g == b
                accepted_old = neutral or not (olo <= b <= ohi)
                accepted_new = neutral or not (nlo <= b <= nhi)
                counts["visited"] += 1
                counts["old_accepted"] += accepted_old
                counts["new_accepted"] += accepted_new
                if accepted_old and not accepted_new:
                    kind = "accept_to_reject"
                elif accepted_new and not accepted_old:
                    kind = "reject_to_accept"
                elif not accepted_old and (olo,ohi) != (nlo,nhi):
                    kind = "rejection_payload_changes"
                else:
                    kind = "unchanged"
                counts[kind] += 1
                if kind in examples and len(examples[kind]) < 8:
                    examples[kind].append({"rgb":[r,g,b],"old_interval":[olo,ohi],"new_interval":[nlo,nhi]})
    require(counts["visited"] == 256**3, "full cube visited")
    require(counts["visited"] == sum(counts[k] for k in ["accept_to_reject","reject_to_accept","rejection_payload_changes","unchanged"]), "outcome partition")
    result = {"schema":1,"scope":"Exact finite observable delta; no empirical superiority or historical provenance claim", "counts":counts,"examples":examples,"raw_different_columns":sum(old[k:k+2] != new[k:k+2] for k in range(0,len(old),2)),"raw_different_bytes":sum(a!=b for a,b in zip(old,new)),"old_raw_sha256":hashlib.sha256(old).hexdigest(),"new_raw_sha256":hashlib.sha256(new).hexdigest(),"new_codec_bytes":len(new_codec),"new_codec_records":(len(new_codec)-522)//3,"new_codec_sha256":hashlib.sha256(new_codec).hexdigest()}
    args.output.joinpath("delta.json").write_text(json.dumps(result,indent=2)+"\n", encoding='utf-8', newline='\n')
    args.output.joinpath("new-column-rle.bin").write_bytes(new_codec)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
