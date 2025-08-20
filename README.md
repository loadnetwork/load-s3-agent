## About
`s3-load-agent` is a data agent built on top of HyperBEAM `~s3@1.0` temporal data storage device. This agent orchestrates the location of the data moving it from temporal to permanent (Arweave).

> N.B: beta testing release, unstable and subject to breaking changes, use in testing enviroments only.

## Agent API

- GET `/` : agent info
- GET `/stats` : storage stats
- GET `/:dataitem_id` : generate a presigned get_object URL to access the ANS-104 DataItem data.
- POST `/upload` : post data to store a DataItem offchain on `~s3@1.0`

```bash
echo -n "hello world" | curl -X POST https://load-s3-agent.load.network/upload \
  -H "Authorization: Bearer REACH_OUT_TO_US" \
  -F "file=@-;type=text/plain" \
  -F "content_type=text/plain"
```

## License
This agent is licensed under the [MIT License](./LICENSE)