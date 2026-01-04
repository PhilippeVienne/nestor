resource "aws_kms_key" "ecr" {
  count = var.enable_kms_encryption ? 1 : 0

  description             = "KMS key for ECR encryption"
  deletion_window_in_days = var.kms_deletion_window_days
  enable_key_rotation     = true

  tags = {
    Name = "${local.name_prefix}-ecr-kms"
  }
}

resource "aws_kms_alias" "ecr" {
  count = var.enable_kms_encryption ? 1 : 0

  name          = "alias/${local.name_prefix}-ecr"
  target_key_id = aws_kms_key.ecr[0].key_id
}

output "kms_key_id" {
  description = "KMS key ID for ECR encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.ecr[0].id : null
}

output "kms_key_arn" {
  description = "KMS key ARN for ECR encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.ecr[0].arn : null
}

# KMS Key for S3 Audio Bucket
resource "aws_kms_key" "s3_audio" {
  count = var.enable_kms_encryption ? 1 : 0

  description             = "KMS key for S3 audio recordings encryption"
  deletion_window_in_days = var.kms_deletion_window_days
  enable_key_rotation     = true

  tags = {
    Name = "${local.name_prefix}-s3-audio-kms"
  }
}

resource "aws_kms_alias" "s3_audio" {
  count = var.enable_kms_encryption ? 1 : 0

  name          = "alias/${local.name_prefix}-s3-audio"
  target_key_id = aws_kms_key.s3_audio[0].key_id
}

# KMS Key for S3 Transcripts Bucket
resource "aws_kms_key" "s3_transcripts" {
  count = var.enable_kms_encryption ? 1 : 0

  description             = "KMS key for S3 transcripts encryption"
  deletion_window_in_days = var.kms_deletion_window_days
  enable_key_rotation     = true

  tags = {
    Name = "${local.name_prefix}-s3-transcripts-kms"
  }
}

resource "aws_kms_alias" "s3_transcripts" {
  count = var.enable_kms_encryption ? 1 : 0

  name          = "alias/${local.name_prefix}-s3-transcripts"
  target_key_id = aws_kms_key.s3_transcripts[0].key_id
}

output "kms_s3_audio_key_id" {
  description = "KMS key ID for S3 audio bucket encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.s3_audio[0].id : null
}

output "kms_s3_audio_key_arn" {
  description = "KMS key ARN for S3 audio bucket encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.s3_audio[0].arn : null
}

output "kms_s3_transcripts_key_id" {
  description = "KMS key ID for S3 transcripts bucket encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.s3_transcripts[0].id : null
}

output "kms_s3_transcripts_key_arn" {
  description = "KMS key ARN for S3 transcripts bucket encryption"
  value       = var.enable_kms_encryption ? aws_kms_key.s3_transcripts[0].arn : null
}
