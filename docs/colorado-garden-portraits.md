# Garden City portrait provenance

The six portraits associated with the August 3, 2026 recorded votes come from individual Town of Garden City official profile pages. The profile name, current role, and image element were checked together; the vote and meeting role come from the minutes separately. A current profile does not establish the role or term on the vote date by itself.

The proof file pins each profile digest and original image digest, dimensions and byte size. The offline helper rechecks the exact profile article, its named role and unique source-linked image, both saved capture receipts and raw bytes, JPEG signature and dimensions. It stages unchanged original bytes, named by SHA-256, and a manifest containing canonical town-profile URLs. CDN query strings stay in the private capture records; they are not copied to the staged manifest.

```sh
python3 scripts/colorado_garden_portraits.py \
  --archive /absolute/owner-only/portrait-archive \
  --output /absolute/fresh/private/staging-directory
python3 scripts/test-colorado-garden-portraits.py
```

The Town's profile pages display a 2026 Town of Garden City copyright notice. No explicit image reuse license was found on those pages. This helper establishes provenance and identity only; the images do not establish an approval or vote.
