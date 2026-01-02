import base64
from cryptography.hazmat.primitives.asymmetric import ed25519
sk = ed25519.Ed25519PrivateKey.generate()
pk = sk.public_key()
print("AEGIS_OPERATOR_SK_B64=" + base64.b64encode(sk.private_bytes_raw()).decode())
print("AEGIS_VERIFYING_PK_B64=" + base64.b64encode(pk.public_bytes_raw()).decode())