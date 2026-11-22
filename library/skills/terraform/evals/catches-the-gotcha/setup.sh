#!/bin/sh
set -e
cat > main.tf <<'EOF'
variable "environment" { type = string }
variable "name" { type = string }

resource "aws_s3_bucket" "this" {
  bucket = "${var.environment}-${var.name}"
}
EOF
