locals {
  parameter_prefix = "${var.parameter_store_prefix}-0-base"
}

# ECR Outputs
resource "aws_ssm_parameter" "ecr_repository_url" {
  name        = "${local.parameter_prefix}/ecr_repository_url"
  description = "ECR repository URL"
  type        = "SecureString"
  value       = module.ecr_repository.repository_url

  tags = {
    Name = "${local.parameter_prefix}/ecr_repository_url"
  }
}

resource "aws_ssm_parameter" "ecr_repository_arn" {
  name        = "${local.parameter_prefix}/ecr_repository_arn"
  description = "ECR repository ARN"
  type        = "SecureString"
  value       = module.ecr_repository.repository_arn

  tags = {
    Name = "${local.parameter_prefix}/ecr_repository_arn"
  }
}

resource "aws_ssm_parameter" "ecr_repository_name" {
  name        = "${local.parameter_prefix}/ecr_repository_name"
  description = "ECR repository name"
  type        = "SecureString"
  value       = module.ecr_repository.repository_name

  tags = {
    Name = "${local.parameter_prefix}/ecr_repository_name"
  }
}

# S3 Outputs
resource "aws_ssm_parameter" "s3_audio_bucket_name" {
  name        = "${local.parameter_prefix}/s3_audio_bucket_name"
  description = "S3 audio recordings bucket name"
  type        = "SecureString"
  value       = aws_s3_bucket.audio_recordings.id

  tags = {
    Name = "${local.parameter_prefix}/s3_audio_bucket_name"
  }
}

resource "aws_ssm_parameter" "s3_audio_bucket_arn" {
  name        = "${local.parameter_prefix}/s3_audio_bucket_arn"
  description = "S3 audio recordings bucket ARN"
  type        = "SecureString"
  value       = aws_s3_bucket.audio_recordings.arn

  tags = {
    Name = "${local.parameter_prefix}/s3_audio_bucket_arn"
  }
}

resource "aws_ssm_parameter" "s3_transcript_bucket_name" {
  name        = "${local.parameter_prefix}/s3_transcript_bucket_name"
  description = "S3 transcripts bucket name"
  type        = "SecureString"
  value       = aws_s3_bucket.transcripts.id

  tags = {
    Name = "${local.parameter_prefix}/s3_transcript_bucket_name"
  }
}

resource "aws_ssm_parameter" "s3_transcript_bucket_arn" {
  name        = "${local.parameter_prefix}/s3_transcript_bucket_arn"
  description = "S3 transcripts bucket ARN"
  type        = "SecureString"
  value       = aws_s3_bucket.transcripts.arn

  tags = {
    Name = "${local.parameter_prefix}/s3_transcript_bucket_arn"
  }
}

# IAM Policy ARNs - ECR
resource "aws_ssm_parameter" "iam_policy_ecr_push_arn" {
  name        = "${local.parameter_prefix}/iam_policy_ecr_push_arn"
  description = "IAM policy ARN for ECR push access"
  type        = "SecureString"
  value       = module.ecr_repository.iam_policy_ecr_push_arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_ecr_push_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_ecr_pull_arn" {
  name        = "${local.parameter_prefix}/iam_policy_ecr_pull_arn"
  description = "IAM policy ARN for ECR pull access"
  type        = "SecureString"
  value       = module.ecr_repository.iam_policy_ecr_pull_arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_ecr_pull_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_ecr_admin_arn" {
  name        = "${local.parameter_prefix}/iam_policy_ecr_admin_arn"
  description = "IAM policy ARN for ECR admin access"
  type        = "SecureString"
  value       = module.ecr_repository.iam_policy_ecr_admin_arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_ecr_admin_arn"
  }
}

# IAM Policy ARNs - S3
resource "aws_ssm_parameter" "iam_policy_s3_audio_write_arn" {
  name        = "${local.parameter_prefix}/iam_policy_s3_audio_write_arn"
  description = "IAM policy ARN for S3 audio write access"
  type        = "SecureString"
  value       = aws_iam_policy.s3_audio_write.arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_s3_audio_write_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_s3_audio_read_arn" {
  name        = "${local.parameter_prefix}/iam_policy_s3_audio_read_arn"
  description = "IAM policy ARN for S3 audio read access"
  type        = "SecureString"
  value       = aws_iam_policy.s3_audio_read.arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_s3_audio_read_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_s3_transcripts_write_arn" {
  name        = "${local.parameter_prefix}/iam_policy_s3_transcripts_write_arn"
  description = "IAM policy ARN for S3 transcripts write access"
  type        = "SecureString"
  value       = aws_iam_policy.s3_transcripts_write.arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_s3_transcripts_write_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_s3_transcripts_read_arn" {
  name        = "${local.parameter_prefix}/iam_policy_s3_transcripts_read_arn"
  description = "IAM policy ARN for S3 transcripts read access"
  type        = "SecureString"
  value       = aws_iam_policy.s3_transcripts_read.arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_s3_transcripts_read_arn"
  }
}

# KMS Key ARNs (conditional)
resource "aws_ssm_parameter" "kms_ecr_key_arn" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.parameter_prefix}/kms_ecr_key_arn"
  description = "KMS key ARN for ECR encryption"
  type        = "SecureString"
  value       = module.ecr_repository.kms_key_arn

  tags = {
    Name = "${local.parameter_prefix}/kms_ecr_key_arn"
  }
}

resource "aws_ssm_parameter" "kms_s3_audio_key_arn" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.parameter_prefix}/kms_s3_audio_key_arn"
  description = "KMS key ARN for S3 audio encryption"
  type        = "SecureString"
  value       = aws_kms_key.s3_audio[0].arn

  tags = {
    Name = "${local.parameter_prefix}/kms_s3_audio_key_arn"
  }
}

resource "aws_ssm_parameter" "kms_s3_transcripts_key_arn" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.parameter_prefix}/kms_s3_transcripts_key_arn"
  description = "KMS key ARN for S3 transcripts encryption"
  type        = "SecureString"
  value       = aws_kms_key.s3_transcripts[0].arn

  tags = {
    Name = "${local.parameter_prefix}/kms_s3_transcripts_key_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_ecr_kms_arn" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.parameter_prefix}/iam_policy_ecr_kms_arn"
  description = "IAM policy ARN for ECR KMS operations"
  type        = "SecureString"
  value       = module.ecr_repository.iam_policy_ecr_kms_arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_ecr_kms_arn"
  }
}

resource "aws_ssm_parameter" "iam_policy_s3_kms_arn" {
  count = var.enable_kms_encryption ? 1 : 0

  name        = "${local.parameter_prefix}/iam_policy_s3_kms_arn"
  description = "IAM policy ARN for S3 KMS operations"
  type        = "SecureString"
  value       = aws_iam_policy.s3_kms[0].arn

  tags = {
    Name = "${local.parameter_prefix}/iam_policy_s3_kms_arn"
  }
}

# VPC Outputs
resource "aws_ssm_parameter" "vpc_id" {
  name        = "${local.parameter_prefix}/vpc_id"
  description = "VPC ID"
  type        = "SecureString"
  value       = module.vpc.vpc_id

  tags = {
    Name = "${local.parameter_prefix}/vpc_id"
  }
}

resource "aws_ssm_parameter" "vpc_cidr_block" {
  name        = "${local.parameter_prefix}/vpc_cidr_block"
  description = "VPC CIDR block"
  type        = "SecureString"
  value       = module.vpc.vpc_cidr_block

  tags = {
    Name = "${local.parameter_prefix}/vpc_cidr_block"
  }
}

resource "aws_ssm_parameter" "vpc_ipv6_cidr_block" {
  name        = "${local.parameter_prefix}/vpc_ipv6_cidr_block"
  description = "VPC IPv6 CIDR block"
  type        = "SecureString"
  value       = module.vpc.vpc_ipv6_cidr_block

  tags = {
    Name = "${local.parameter_prefix}/vpc_ipv6_cidr_block"
  }
}

resource "aws_ssm_parameter" "private_subnets" {
  name        = "${local.parameter_prefix}/private_subnets"
  description = "Private subnet IDs (comma-separated)"
  type        = "SecureString"
  value       = join(",", module.vpc.private_subnets)

  tags = {
    Name = "${local.parameter_prefix}/private_subnets"
  }
}

resource "aws_ssm_parameter" "public_subnets" {
  name        = "${local.parameter_prefix}/public_subnets"
  description = "Public subnet IDs (comma-separated)"
  type        = "SecureString"
  value       = join(",", module.vpc.public_subnets)

  tags = {
    Name = "${local.parameter_prefix}/public_subnets"
  }
}

resource "aws_ssm_parameter" "private_subnet_ipv6_cidr_blocks" {
  name        = "${local.parameter_prefix}/private_subnet_ipv6_cidr_blocks"
  description = "Private subnet IPv6 CIDR blocks (comma-separated)"
  type        = "SecureString"
  value       = join(",", module.vpc.private_subnets_ipv6_cidr_blocks)

  tags = {
    Name = "${local.parameter_prefix}/private_subnet_ipv6_cidr_blocks"
  }
}

resource "aws_ssm_parameter" "public_subnet_ipv6_cidr_blocks" {
  name        = "${local.parameter_prefix}/public_subnet_ipv6_cidr_blocks"
  description = "Public subnet IPv6 CIDR blocks (comma-separated)"
  type        = "SecureString"
  value       = join(",", module.vpc.public_subnets_ipv6_cidr_blocks)

  tags = {
    Name = "${local.parameter_prefix}/public_subnet_ipv6_cidr_blocks"
  }
}

resource "aws_ssm_parameter" "vpc_default_security_group_id" {
  name        = "${local.parameter_prefix}/vpc_default_security_group_id"
  description = "VPC default security group ID"
  type        = "SecureString"
  value       = module.vpc.default_security_group_id

  tags = {
    Name = "${local.parameter_prefix}/vpc_default_security_group_id"
  }
}

# Output the parameter prefix for reference
output "parameter_store_prefix" {
  description = "Prefix used for Parameter Store parameters"
  value       = local.parameter_prefix
}
