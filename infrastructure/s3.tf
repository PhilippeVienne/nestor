data "aws_canonical_user_id" "current" {}

# S3 Bucket for Raw Audio Recordings
resource "aws_s3_bucket" "audio_recordings" {
  bucket_prefix = local.s3_audio_bucket_prefix

  tags = {
    Name = local.s3_audio_bucket_prefix
    Type = "audio-recordings"
  }
}

resource "aws_s3_bucket_versioning" "audio_recordings" {
  bucket = aws_s3_bucket.audio_recordings.id

  versioning_configuration {
    status = var.s3_versioning_enabled ? "Enabled" : "Disabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "audio_recordings" {
  bucket = aws_s3_bucket.audio_recordings.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = var.enable_kms_encryption ? "aws:kms" : "AES256"
      kms_master_key_id = var.enable_kms_encryption ? aws_kms_key.s3_audio[0].arn : null
    }
    bucket_key_enabled = var.enable_kms_encryption
  }
}

resource "aws_s3_bucket_public_access_block" "audio_recordings" {
  count = var.enable_s3_public_access_block ? 1 : 0

  bucket = aws_s3_bucket.audio_recordings.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "audio_recordings" {
  count = length(local.s3_audio_lifecycle_rules) > 0 ? 1 : 0

  bucket = aws_s3_bucket.audio_recordings.id

  dynamic "rule" {
    for_each = local.s3_audio_lifecycle_rules
    content {
      id     = rule.value.id
      status = rule.value.status

      dynamic "transition" {
        for_each = rule.value.transition
        content {
          days          = transition.value.days
          storage_class = transition.value.storage_class
        }
      }

      dynamic "expiration" {
        for_each = rule.value.expiration != null ? [rule.value.expiration] : []
        content {
          days = expiration.value.days
        }
      }
    }
  }
}

# S3 Bucket for Transcripts
resource "aws_s3_bucket" "transcripts" {
  bucket_prefix = local.s3_transcript_bucket_prefix

  tags = {
    Name = local.s3_transcript_bucket_prefix
    Type = "transcripts"
  }
}

resource "aws_s3_bucket_versioning" "transcripts" {
  bucket = aws_s3_bucket.transcripts.id

  versioning_configuration {
    status = var.s3_versioning_enabled ? "Enabled" : "Disabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "transcripts" {
  bucket = aws_s3_bucket.transcripts.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = var.enable_kms_encryption ? "aws:kms" : "AES256"
      kms_master_key_id = var.enable_kms_encryption ? aws_kms_key.s3_transcripts[0].arn : null
    }
    bucket_key_enabled = var.enable_kms_encryption
  }
}

resource "aws_s3_bucket_public_access_block" "transcripts" {
  count = var.enable_s3_public_access_block ? 1 : 0

  bucket = aws_s3_bucket.transcripts.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "transcripts" {
  count = length(local.s3_transcript_lifecycle_rules) > 0 ? 1 : 0

  bucket = aws_s3_bucket.transcripts.id

  dynamic "rule" {
    for_each = local.s3_transcript_lifecycle_rules
    content {
      id     = rule.value.id
      status = rule.value.status

      dynamic "transition" {
        for_each = rule.value.transition
        content {
          days          = transition.value.days
          storage_class = transition.value.storage_class
        }
      }

      dynamic "expiration" {
        for_each = rule.value.expiration != null ? [rule.value.expiration] : []
        content {
          days = expiration.value.days
        }
      }
    }
  }
}

# Outputs
output "s3_audio_bucket_name" {
  description = "Name of the S3 bucket for audio recordings"
  value       = aws_s3_bucket.audio_recordings.id
}

output "s3_audio_bucket_arn" {
  description = "ARN of the S3 bucket for audio recordings"
  value       = aws_s3_bucket.audio_recordings.arn
}

output "s3_transcript_bucket_name" {
  description = "Name of the S3 bucket for transcripts"
  value       = aws_s3_bucket.transcripts.id
}

output "s3_transcript_bucket_arn" {
  description = "ARN of the S3 bucket for transcripts"
  value       = aws_s3_bucket.transcripts.arn
}
