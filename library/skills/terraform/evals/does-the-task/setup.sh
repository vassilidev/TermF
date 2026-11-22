#!/bin/sh
set -e
cat > main.tf <<'EOF'
terraform {
  required_version = ">= 1.11"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 6.0" }
  }
}

resource "aws_db_instance" "main" {
  identifier        = "app"
  engine            = "postgres"
  instance_class    = "db.t4g.micro"
  allocated_storage = 20
  username          = "app"
}
EOF
