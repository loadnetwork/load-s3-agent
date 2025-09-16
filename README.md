## About
`s3-load-agent` is a data agent built on top of HyperBEAM `~s3@1.0` temporal data storage device. This agent orchestrates the location of the data moving it from temporal to permanent (Arweave).

> N.B: beta testing release, unstable and subject to breaking changes, use in testing enviroments only.

## Agent API

- GET `/` : agent info
- GET `/stats` : storage stats
- GET `/:dataitem_id` : generate a presigned get_object URL to access the ANS-104 DataItem data.
- POST `/upload` : post data to store a public offchain DataItem on `~s3@1.0`
- POST `/upload/private` : post data to store a private offchain DataItem on `~s3@1.0`
- POST `/post/:dataitem_id` : post an `~s3@1.0` DataItem to Arweave via Turbo (N.B: Turbo covers any dataitem cost with size <= 100KB).

### Upload data and return an agent public signed DataItem
```bash
echo -n "hello world" | curl -X POST https://load-s3-agent.load.network/upload \
  -H "Authorization: Bearer REACH_OUT_TO_US" \
  -F "file=@-;type=text/plain" \
  -F "content_type=text/plain"
```

### Upload data and return an agent private signed DataItem
```bash
echo -n "hello world" | curl -X POST https://load-s3-agent.load.network/upload/private \
  -H "Authorization: Bearer $load_acc_api_key" \
  -H "bucket_name: $bucket_name" \
  -F "file=@-;type=text/plain" \
  -F "content_type=text/plain"
```

### Upload a signed DataItem and store it in Load S3

```bash
curl -X POST https://load-s3-agent.load.network/upload \
  -H "Authorization: Bearer REACH_OUT_TO_US" \
  -H "signed: true" \
  -F "file=@your-signed-dataitem.bin"
```

### Post offchain DataItem to Arweave

for offchain dataitem `eoNAO-HlYasHJt3QFDuRrMVdLUxq5B8bXe4N_kboNWs`

```bash
curl -X POST \
  "https://load-s3-agent.load.network/post/eoNAO-HlYasHJt3QFDuRrMVdLUxq5B8bXe4N_kboNWs" \
  -H "Authorization: Bearer REACH_OUT_TO_US" \
  -H "Content-Type: application/json"
```

## License
This agent is licensed under the [MIT License](./LICENSE)