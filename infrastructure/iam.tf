data "aws_caller_identity" "current" {}

# IAM Policy for S3 Audio Bucket Write Access
resource "aws_iam_policy" "s3_audio_write" {
  name        = "${local.name_prefix}-s3-audio-write"
  description = "Policy to allow writing to S3 audio recordings bucket"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "S3AudioBucketWrite"
        Effect = "Allow"
        Action = [
          "s3:PutObject",
          "s3:PutObjectAcl",
          "s3:GetObject",
          "s3:GetObjectVersion",
          "s3:DeleteObject"
        ]
        Resource = "${aws_s3_bucket.audio_recordings.arn}/*"
      },
      {
        Sid    = "S3AudioBucketList"
        Effect = "Allow"
        Action = [
          "s3:ListBucket",
          "s3:GetBucketLocation"
        ]
        Resource = aws_s3_bucket.audio_recordings.arn
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-s3-audio-write"
  }
}

# IAM Policy for S3 Audio Bucket Read Access
resource "aws_iam_policy" "s3_audio_read" {
  name        = "${local.name_prefix}-s3-audio-read"
  description = "Policy to allow reading from S3 audio recordings bucket"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "S3AudioBucketRead"
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:GetObjectVersion"
        ]
        Resource = "${aws_s3_bucket.audio_recordings.arn}/*"
      },
      {
        Sid    = "S3AudioBucketList"
        Effect = "Allow"
        Action = [
          "s3:ListBucket",
          "s3:GetBucketLocation"
        ]
        Resource = aws_s3_bucket.audio_recordings.arn
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-s3-audio-read"
  }
}

# IAM Policy for S3 Transcripts Bucket Write Access
resource "aws_iam_policy" "s3_transcripts_write" {
  name        = "${local.name_prefix}-s3-transcripts-write"
  description = "Policy to allow writing to S3 transcripts bucket"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "S3TranscriptsBucketWrite"
        Effect = "Allow"
        Action = [
          "s3:PutObject",
          "s3:PutObjectAcl",
          "s3:GetObject",
          "s3:GetObjectVersion",
          "s3:DeleteObject"
        ]
        Resource = "${aws_s3_bucket.transcripts.arn}/*"
      },
      {
        Sid    = "S3TranscriptsBucketList"
        Effect = "Allow"
        Action = [
          "s3:ListBucket",
          "s3:GetBucketLocation"
        ]
        Resource = aws_s3_bucket.transcripts.arn
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-s3-transcripts-write"
  }
}

# IAM Policy for S3 Transcripts Bucket Read Access
resource "aws_iam_policy" "s3_transcripts_read" {
  name        = "${local.name_prefix}-s3-transcripts-read"
  description = "Policy to allow reading from S3 transcripts bucket"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "S3TranscriptsBucketRead"
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:GetObjectVersion"
        ]
        Resource = "${aws_s3_bucket.transcripts.arn}/*"
      },
      {
        Sid    = "S3TranscriptsBucketList"
        Effect = "Allow"
        Action = [
          "s3:ListBucket",
          "s3:GetBucketLocation"
        ]
        Resource = aws_s3_bucket.transcripts.arn
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-s3-transcripts-read"
  }
}

# IAM Policy for S3 KMS Access (if enabled)
resource "aws_iam_policy" "s3_kms" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.name_prefix}-s3-kms"
  description = "Policy to allow KMS operations for S3 encryption"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "KMSForS3"
        Effect = "Allow"
        Action = [
          "kms:Decrypt",
          "kms:Encrypt",
          "kms:GenerateDataKey",
          "kms:DescribeKey"
        ]
        Resource = [
          aws_kms_key.s3_audio[0].arn,
          aws_kms_key.s3_transcripts[0].arn
        ]
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-s3-kms"
  }
}

output "iam_policy_s3_audio_write_arn" {
  description = "ARN of the IAM policy for S3 audio bucket write access"
  value       = aws_iam_policy.s3_audio_write.arn
}

output "iam_policy_s3_audio_read_arn" {
  description = "ARN of the IAM policy for S3 audio bucket read access"
  value       = aws_iam_policy.s3_audio_read.arn
}

output "iam_policy_s3_transcripts_write_arn" {
  description = "ARN of the IAM policy for S3 transcripts bucket write access"
  value       = aws_iam_policy.s3_transcripts_write.arn
}

output "iam_policy_s3_transcripts_read_arn" {
  description = "ARN of the IAM policy for S3 transcripts bucket read access"
  value       = aws_iam_policy.s3_transcripts_read.arn
}

output "iam_policy_s3_kms_arn" {
  description = "ARN of the IAM policy for S3 KMS operations"
  value       = var.enable_kms_encryption ? aws_iam_policy.s3_kms[0].arn : null
}
