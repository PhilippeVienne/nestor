locals {
  ecr_repository_name = "${var.project_name}-app"

  # Common resource naming
  name_prefix = "${var.project_name}-${var.environment}"

  # S3 bucket prefixes
  s3_audio_bucket_prefix      = "${var.project_name}-${var.environment}-audio-recordings-"
  s3_transcript_bucket_prefix = "${var.project_name}-${var.environment}-transcripts-"

  # ECR lifecycle policy
  ecr_lifecycle_policy = {
    rules = [
      {
        rulePriority = 1
        description  = "Keep last ${var.ecr_lifecycle_keep_count} tagged images"
        selection = {
          tagStatus     = "tagged"
          tagPrefixList = ["v"]
          countType     = "imageCountMoreThan"
          countNumber   = var.ecr_lifecycle_keep_count
        }
        action = {
          type = "expire"
        }
      },
      {
        rulePriority = 2
        description  = "Remove untagged images after ${var.ecr_untagged_expiry_days} days"
        selection = {
          tagStatus   = "untagged"
          countType   = "sinceImagePushed"
          countUnit   = "days"
          countNumber = var.ecr_untagged_expiry_days
        }
        action = {
          type = "expire"
        }
      }
    ]
  }

  # S3 lifecycle rules for audio bucket
  s3_audio_lifecycle_rules = var.s3_lifecycle_audio_expiry_days > 0 || var.s3_lifecycle_audio_transition_days > 0 ? [
    {
      id     = "audio-lifecycle"
      status = "Enabled"

      transition = var.s3_lifecycle_audio_transition_days > 0 ? [
        {
          days          = var.s3_lifecycle_audio_transition_days
          storage_class = "INTELLIGENT_TIERING"
        }
      ] : []

      expiration = var.s3_lifecycle_audio_expiry_days > 0 ? {
        days = var.s3_lifecycle_audio_expiry_days
      } : null
    }
  ] : []

  # S3 lifecycle rules for transcript bucket
  s3_transcript_lifecycle_rules = var.s3_lifecycle_transcript_expiry_days > 0 || var.s3_lifecycle_transcript_transition_days > 0 ? [
    {
      id     = "transcript-lifecycle"
      status = "Enabled"

      transition = var.s3_lifecycle_transcript_transition_days > 0 ? [
        {
          days          = var.s3_lifecycle_transcript_transition_days
          storage_class = "INTELLIGENT_TIERING"
        }
      ] : []

      expiration = var.s3_lifecycle_transcript_expiry_days > 0 ? {
        days = var.s3_lifecycle_transcript_expiry_days
      } : null
    }
  ] : []
}
