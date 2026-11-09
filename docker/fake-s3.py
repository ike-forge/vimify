"""Answers stu's ListBuckets and ListObjectsV2 so it starts without AWS. Port in argv[1]."""

import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

NS = "http://s3.amazonaws.com/doc/2006-03-01/"
BUCKETS = ["alpha", "beta"]


def buckets():
    items = "".join(
        f"<Bucket><Name>{b}</Name><CreationDate>2026-01-01T00:00:00.000Z</CreationDate></Bucket>"
        for b in BUCKETS
    )
    return f'<ListAllMyBucketsResult xmlns="{NS}"><Buckets>{items}</Buckets></ListAllMyBucketsResult>'


def objects(bucket):
    return (
        f'<ListBucketResult xmlns="{NS}"><Name>{bucket}</Name><Prefix></Prefix>'
        "<KeyCount>0</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>"
        "</ListBucketResult>"
    )


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?")[0].strip("/")
        body = (objects(path) if path else buckets()).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/xml")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
