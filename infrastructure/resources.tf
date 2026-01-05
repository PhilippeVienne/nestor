# ECR Repository for API Lambda
module "ecr_repository" {
  source = "./modules/ecr"

  repository_name          = local.ecr_repository_name
  image_tag_mutability     = var.ecr_image_tag_mutability
  scan_on_push             = var.ecr_scan_on_push
  enable_kms_encryption    = var.enable_kms_encryption
  kms_deletion_window_days = var.kms_deletion_window_days
  lifecycle_policy         = local.ecr_lifecycle_policy
  create_iam_policies      = true

  tags = {
    Name        = local.ecr_repository_name
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# State migration - moved resources to module
moved {
  from = aws_ecr_repository.app
  to   = module.ecr_repository.aws_ecr_repository.main
}

moved {
  from = aws_ecr_lifecycle_policy.app
  to   = module.ecr_repository.aws_ecr_lifecycle_policy.main
}

moved {
  from = aws_kms_key.ecr[0]
  to   = module.ecr_repository.aws_kms_key.ecr[0]
}

moved {
  from = aws_kms_alias.ecr[0]
  to   = module.ecr_repository.aws_kms_alias.ecr[0]
}

moved {
  from = aws_iam_policy.ecr_push
  to   = module.ecr_repository.aws_iam_policy.ecr_push[0]
}

moved {
  from = aws_iam_policy.ecr_pull
  to   = module.ecr_repository.aws_iam_policy.ecr_pull[0]
}

moved {
  from = aws_iam_policy.ecr_admin
  to   = module.ecr_repository.aws_iam_policy.ecr_admin[0]
}

moved {
  from = aws_iam_policy.ecr_kms[0]
  to   = module.ecr_repository.aws_iam_policy.ecr_kms[0]
}

# SQS Queues for audio processing
module "audio_processing_queue" {
  source = "./modules/sqs"

  queue_name                 = "${local.name_prefix}-audio-processing"
  visibility_timeout_seconds = 900     # 15 minutes for long audio files
  message_retention_seconds  = 1209600 # 14 days
  enable_dlq                 = true
  enable_dlq_alarm           = true

  tags = {
    Name        = "${local.name_prefix}-audio-processing-queue"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# SQS Queue for transcript processing (triggers Bedrock Lambda)
module "transcript_processing_queue" {
  source = "./modules/sqs"

  queue_name                 = "${local.name_prefix}-transcript-processing"
  visibility_timeout_seconds = 300    # 5 minutes
  message_retention_seconds  = 345600 # 4 days
  enable_dlq                 = true
  enable_dlq_alarm           = true

  tags = {
    Name        = "${local.name_prefix}-transcript-processing-queue"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# DynamoDB table for task results
module "task_results_table" {
  source = "./modules/dynamodb"

  table_name   = "${local.name_prefix}-task-results"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "message_id"
  range_key    = "timestamp"

  attributes = [
    {
      name = "message_id"
      type = "S"
    },
    {
      name = "timestamp"
      type = "S"
    },
    {
      name = "task_type"
      type = "S"
    }
  ]

  global_secondary_indexes = [
    {
      name            = "task_type-timestamp-index"
      hash_key        = "task_type"
      range_key       = "timestamp"
      projection_type = "ALL"
    }
  ]

  ttl_enabled        = true
  ttl_attribute_name = "ttl"

  tags = {
    Name        = "${local.name_prefix}-task-results"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# Aurora Serverless PostgreSQL for conversation history
module "postgres_db" {
  source = "./modules/rds"

  identifier                 = "${local.name_prefix}-db"
  vpc_id                     = module.vpc.vpc_id
  subnet_ids                 = module.vpc.private_subnets
  allowed_security_group_ids = [] # Will be populated after Lambda is created

  # Aurora Serverless v2 configuration
  use_aurora_serverless = var.use_aurora_serverless
  aurora_engine_version = var.aurora_engine_version
  aurora_min_capacity   = var.aurora_min_capacity
  aurora_max_capacity   = var.aurora_max_capacity

  # RDS configuration (used if Aurora Serverless is disabled)
  engine_version        = var.rds_engine_version
  instance_class        = var.rds_instance_class
  allocated_storage     = var.rds_allocated_storage
  max_allocated_storage = var.rds_max_allocated_storage

  # Common configuration
  database_name   = var.rds_database_name
  master_username = var.rds_master_username

  backup_retention_period = var.rds_backup_retention_period
  deletion_protection     = var.rds_deletion_protection
  skip_final_snapshot     = var.rds_skip_final_snapshot

  performance_insights_enabled = true
  monitoring_interval          = 60

  tags = {
    Name        = "${local.name_prefix}-database"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# Whisper EC2 Auto Scaling Group
module "whisper_asg" {
  count  = local.whisper_ami_id != null ? 1 : 0
  source = "./modules/whisper-asg"

  name_prefix   = local.name_prefix
  vpc_id        = module.vpc.vpc_id
  subnet_ids    = module.vpc.private_subnets
  ami_id        = local.whisper_ami_id
  instance_type = var.whisper_instance_type

  asg_min_size         = var.whisper_asg_min_size
  asg_max_size         = var.whisper_asg_max_size
  asg_desired_capacity = var.whisper_asg_desired_capacity

  sqs_queue_url  = module.audio_processing_queue.queue_url
  sqs_queue_name = module.audio_processing_queue.queue_name
  sqs_queue_arn  = module.audio_processing_queue.queue_arn

  audio_bucket_name      = aws_s3_bucket.audio_recordings.id
  audio_bucket_arn       = aws_s3_bucket.audio_recordings.arn
  transcript_bucket_name = aws_s3_bucket.transcripts.id
  transcript_bucket_arn  = aws_s3_bucket.transcripts.arn

  aws_region = var.aws_region

  scale_up_threshold   = var.whisper_scale_up_threshold
  scale_down_threshold = var.whisper_scale_down_threshold

  enable_ssh_access = var.whisper_enable_ssh
  ssh_cidr_block    = var.whisper_ssh_cidr

  tags = {
    Name        = "${local.name_prefix}-whisper-asg"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# Lambda for Bedrock NLP processing
module "bedrock_lambda" {
  source = "./modules/lambda-bedrock"

  function_name = "${local.name_prefix}-bedrock-processor"
  image_uri     = "${module.ecr_repository.repository_url}:latest"
  vpc_id        = module.vpc.vpc_id
  subnet_ids    = module.vpc.private_subnets

  transcript_bucket_name = aws_s3_bucket.transcripts.id
  transcript_bucket_arn  = aws_s3_bucket.transcripts.arn
  rds_secret_arn         = module.postgres_db.db_password_secret_arn

  sqs_queue_arn    = module.transcript_processing_queue.queue_arn
  bedrock_model_id = var.bedrock_model_id
  aws_region       = var.aws_region

  timeout     = 300
  memory_size = 1024

  enable_sqs_trigger = true

  tags = {
    Name        = "${local.name_prefix}-bedrock-lambda"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# Allow Lambda to access RDS
resource "aws_vpc_security_group_ingress_rule" "rds_from_lambda" {
  security_group_id            = module.postgres_db.db_security_group_id
  referenced_security_group_id = module.bedrock_lambda.security_group_id
  from_port                    = 5432
  to_port                      = 5432
  ip_protocol                  = "tcp"
  description                  = "PostgreSQL access from Bedrock Lambda"
}

# GitHub Actions OIDC
module "github_oidc" {
  source = "./modules/github-oidc"

  role_name_prefix    = "${local.name_prefix}-github-actions-"
  github_repositories = var.github_repositories

  enable_ecr_access   = true
  ecr_repository_arns = [module.ecr_repository.repository_arn]

  enable_s3_access = true
  s3_bucket_arns = [
    aws_s3_bucket.audio_recordings.arn,
    aws_s3_bucket.transcripts.arn
  ]

  enable_lambda_deploy = true
  lambda_function_arns = [module.bedrock_lambda.function_arn]

  enable_terraform_state     = true
  terraform_state_bucket_arn = "arn:aws:s3:::${var.terraform_state_bucket_name}"

  custom_policies = {
    terraform_full_access = jsonencode({
      Version = "2012-10-17"
      Statement = [
        {
          Effect = "Allow"
          Action = [
            "ec2:*",
            "rds:*",
            "dynamodb:*",
            "sqs:*",
            "secretsmanager:*",
            "ssm:*",
            "iam:*",
            "kms:*",
            "logs:*",
            "cloudwatch:*",
            "autoscaling:*",
            "elasticloadbalancing:*"
          ]
          Resource = "*"
        }
      ]
    })
  }

  tags = {
    Name        = "${local.name_prefix}-github-oidc"
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# Outputs for the modules
output "ecr_repository_url" {
  description = "URL of the ECR repository"
  value       = module.ecr_repository.repository_url
}

output "ecr_repository_arn" {
  description = "ARN of the ECR repository"
  value       = module.ecr_repository.repository_arn
}

output "ecr_repository_name" {
  description = "Name of the ECR repository"
  value       = module.ecr_repository.repository_name
}

output "iam_policy_ecr_push_arn" {
  description = "ARN of the IAM policy for ECR push access"
  value       = module.ecr_repository.iam_policy_ecr_push_arn
}

output "iam_policy_ecr_pull_arn" {
  description = "ARN of the IAM policy for ECR pull access"
  value       = module.ecr_repository.iam_policy_ecr_pull_arn
}

output "iam_policy_ecr_admin_arn" {
  description = "ARN of the IAM policy for ECR admin access"
  value       = module.ecr_repository.iam_policy_ecr_admin_arn
}

output "iam_policy_ecr_kms_arn" {
  description = "ARN of the IAM policy for ECR KMS operations"
  value       = module.ecr_repository.iam_policy_ecr_kms_arn
}

output "audio_processing_queue_url" {
  description = "URL of the audio processing SQS queue"
  value       = module.audio_processing_queue.queue_url
}

output "transcript_processing_queue_url" {
  description = "URL of the transcript processing SQS queue"
  value       = module.transcript_processing_queue.queue_url
}

output "task_results_table_name" {
  description = "Name of the DynamoDB task results table"
  value       = module.task_results_table.table_name
}

output "postgres_endpoint" {
  description = "PostgreSQL connection endpoint"
  value       = module.postgres_db.db_instance_endpoint
}

output "postgres_secret_arn" {
  description = "ARN of the secret containing PostgreSQL credentials"
  value       = module.postgres_db.db_password_secret_arn
}

output "whisper_asg_name" {
  description = "Name of the Whisper Auto Scaling Group"
  value       = length(module.whisper_asg) > 0 ? module.whisper_asg[0].asg_name : null
}

output "bedrock_lambda_arn" {
  description = "ARN of the Bedrock Lambda function"
  value       = module.bedrock_lambda.function_arn
}

output "github_actions_role_arn" {
  description = "ARN of the GitHub Actions IAM role"
  value       = module.github_oidc.role_arn
  sensitive   = false
}
