variable "project_name" {
  description = "Name of the project"
  type        = string
  default     = "nestor"
}

variable "environment" {
  description = "Environment name (e.g., dev, staging, production)"
  type        = string
  default     = "production"
}

variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "ecr_image_tag_mutability" {
  description = "Image tag mutability setting for ECR repository"
  type        = string
  default     = "MUTABLE"
  validation {
    condition     = contains(["MUTABLE", "IMMUTABLE"], var.ecr_image_tag_mutability)
    error_message = "Image tag mutability must be either MUTABLE or IMMUTABLE."
  }
}

variable "ecr_scan_on_push" {
  description = "Enable image scanning on push"
  type        = bool
  default     = true
}

variable "ecr_lifecycle_keep_count" {
  description = "Number of tagged images to keep"
  type        = number
  default     = 10
}

variable "ecr_untagged_expiry_days" {
  description = "Days after which untagged images expire"
  type        = number
  default     = 7
}

variable "enable_kms_encryption" {
  description = "Use KMS encryption instead of AES256"
  type        = bool
  default     = false
}

variable "kms_key_id" {
  description = "KMS key ID for ECR encryption (if enable_kms_encryption is true and using external key)"
  type        = string
  default     = null
}

variable "kms_deletion_window_days" {
  description = "Number of days before KMS key deletion (7-30 days)"
  type        = number
  default     = 30
  validation {
    condition     = var.kms_deletion_window_days >= 7 && var.kms_deletion_window_days <= 30
    error_message = "KMS deletion window must be between 7 and 30 days."
  }
}

# S3 Configuration
variable "s3_versioning_enabled" {
  description = "Enable versioning for S3 buckets"
  type        = bool
  default     = true
}

variable "s3_lifecycle_audio_transition_days" {
  description = "Number of days before transitioning audio files to Intelligent-Tiering"
  type        = number
  default     = 90
}

variable "s3_lifecycle_audio_expiry_days" {
  description = "Number of days before expiring audio files (0 = never expire)"
  type        = number
  default     = 0
}

variable "s3_lifecycle_transcript_transition_days" {
  description = "Number of days before transitioning transcript files to Intelligent-Tiering"
  type        = number
  default     = 180
}

variable "s3_lifecycle_transcript_expiry_days" {
  description = "Number of days before expiring transcript files (0 = never expire)"
  type        = number
  default     = 0
}

variable "enable_s3_public_access_block" {
  description = "Enable public access block for S3 buckets"
  type        = bool
  default     = true
}

# Parameter Store Configuration
variable "parameter_store_prefix" {
  description = "Prefix for AWS Systems Manager Parameter Store parameters"
  type        = string
  default     = "/nestor"
}

# VPC Configuration
variable "vpc_cidr" {
  description = "CIDR block for VPC (used for IPv4)"
  type        = string
  default     = "10.0.0.0/16"
}

variable "vpc_azs" {
  description = "Availability zones for VPC subnets"
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b", "us-east-1c"]
}

variable "vpc_private_subnets" {
  description = "Private subnet CIDR blocks"
  type        = list(string)
  default     = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
}

variable "vpc_public_subnets" {
  description = "Public subnet CIDR blocks"
  type        = list(string)
  default     = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
}

variable "vpc_enable_ipv6" {
  description = "Enable IPv6 for VPC"
  type        = bool
  default     = true
}

variable "vpc_enable_nat_gateway" {
  description = "Enable NAT Gateway for VPC"
  type        = bool
  default     = false
}

variable "vpc_enable_dns_hostnames" {
  description = "Enable DNS hostnames in VPC"
  type        = bool
  default     = true
}

variable "vpc_enable_dns_support" {
  description = "Enable DNS support in VPC"
  type        = bool
  default     = true
}

# Database Configuration
variable "use_aurora_serverless" {
  description = "Use Aurora Serverless v2 instead of RDS (cost-optimized with scale to zero)"
  type        = bool
  default     = true
}

variable "aurora_engine_version" {
  description = "Aurora PostgreSQL engine version"
  type        = string
  default     = "16.2"
}

variable "aurora_min_capacity" {
  description = "Minimum Aurora capacity units (0.5 for scale to zero)"
  type        = number
  default     = 0.5
}

variable "aurora_max_capacity" {
  description = "Maximum Aurora capacity units"
  type        = number
  default     = 1
}

# RDS Configuration (used if Aurora Serverless is disabled)
variable "rds_engine_version" {
  description = "PostgreSQL engine version (for non-Aurora RDS)"
  type        = string
  default     = "16.3"
}

variable "rds_instance_class" {
  description = "RDS instance class (for non-Aurora RDS)"
  type        = string
  default     = "db.t3.micro"
}

variable "rds_allocated_storage" {
  description = "Initial allocated storage in GB (for non-Aurora RDS)"
  type        = number
  default     = 20
}

variable "rds_max_allocated_storage" {
  description = "Maximum allocated storage for autoscaling (for non-Aurora RDS)"
  type        = number
  default     = 100
}

variable "rds_database_name" {
  description = "Name of the default database"
  type        = string
  default     = "nestor"
}

variable "rds_master_username" {
  description = "Master username for RDS"
  type        = string
  default     = "postgres"
}

variable "rds_backup_retention_period" {
  description = "Backup retention period in days"
  type        = number
  default     = 7
}

variable "rds_deletion_protection" {
  description = "Enable deletion protection"
  type        = bool
  default     = true
}

variable "rds_skip_final_snapshot" {
  description = "Skip final snapshot when destroying"
  type        = bool
  default     = false
}

# Whisper EC2 Configuration
variable "whisper_ami_id" {
  description = "AMI ID for Whisper EC2 instances (leave empty to use latest)"
  type        = string
  default     = ""
}

variable "whisper_instance_type" {
  description = "EC2 instance type for Whisper processing"
  type        = string
  default     = "t3.medium"
}

variable "whisper_asg_min_size" {
  description = "Minimum size of Whisper ASG"
  type        = number
  default     = 1
}

variable "whisper_asg_max_size" {
  description = "Maximum size of Whisper ASG"
  type        = number
  default     = 5
}

variable "whisper_asg_desired_capacity" {
  description = "Desired capacity of Whisper ASG"
  type        = number
  default     = 1
}

variable "whisper_scale_up_threshold" {
  description = "SQS queue depth to trigger scale up"
  type        = number
  default     = 5
}

variable "whisper_scale_down_threshold" {
  description = "SQS queue depth to trigger scale down"
  type        = number
  default     = 1
}

variable "whisper_enable_ssh" {
  description = "Enable SSH access to Whisper instances"
  type        = bool
  default     = false
}

variable "whisper_ssh_cidr" {
  description = "CIDR block for SSH access"
  type        = string
  default     = "0.0.0.0/0"
}

# Bedrock Configuration
variable "bedrock_model_id" {
  description = "Bedrock model ID for NLP processing"
  type        = string
  default     = "anthropic.claude-3-sonnet-20240229-v1:0"
}

# GitHub Actions Configuration
variable "github_repositories" {
  description = "List of GitHub repositories for OIDC (format: owner/repo)"
  type        = list(string)
  default     = []
}

variable "terraform_state_bucket_name" {
  description = "Name of the Terraform state bucket"
  type        = string
  default     = ""
}
