#!/usr/bin/env python3
# /// script
# dependencies = [
#   "aiocoap >= 0.4.11, < 0.5",
#   "cbor2 >= 5.8.0, < 6.0",
#   "cbor-diag >= 1.0.0, < 2.0",
#   "lakers-python >= 0.6.0,",
#   "cryptography >= 46.0.0, < 47.0",
#   "filelock >= 3.24.0, < 4.0",
# ]
# ///
import cbor2
import cbor_diag
from aiocoap import edhoc

keyfile = "server.cosekey"
diagfile = "server.diag"

# keyfile = "testkey.cosekey"
# diagfile = "testdiag.diag"

kid = "1a"

key = edhoc.CoseKeyForEdhoc.generate(keyfile)


public = key.as_ccs(kid, "")
credentials = {
    "coap://*": {
        "edhoc-oscore": {
            "suite": 2,
            "method": 3,
            "own_cred_style": "by-key-id",
            "peer_cred": {"unauthenticated": True},
            "own_cred": public,
            "private_key_file": keyfile,
        }
    }
}

with open(diagfile, "w") as file:
    file.write(
        cbor_diag.cbor2diag(cbor2.dumps(credentials, canonical=True), pretty=True)
    )

print(cbor_diag.cbor2diag(cbor2.dumps(public[14], canonical=True), pretty=False))
