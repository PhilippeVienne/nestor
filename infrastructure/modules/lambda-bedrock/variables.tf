variable "function_name" {
  description = "Name of the Lambda function"
  type        = string
}

variable "image_uri" {
  description = "URI of the container image in ECR"
  type        = string
}

variable "timeout" {
  description = "Lambda timeout in seconds"
  type        = number
  default     = 300
}

variable "memory_size" {
  description = "Lambda memory size in MB"
  type        = number
  default     = 512
}

variable "vpc_id" {
  description = "VPC ID"
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for Lambda"
  type        = list(string)
}

variable "transcript_bucket_name" {
  description = "Name of the transcripts bucket"
  type        = string
}

variable "transcript_bucket_arn" {
  description = "ARN of the transcripts bucket"
  type        = string
}

variable "rds_secret_arn" {
  description = "ARN of the RDS credentials secret"
  type        = string
}

variable "bedrock_model_id" {
  description = "Bedrock model ID"
  type        = string
  default     = "anthropic.claude-3-sonnet-20240229-v1:0"
}

variable "aws_region" {
  description = "AWS region"
  type        = string
}

variable "environment_variables" {
  description = "Additional environment variables"
  type        = map(string)
  default     = {}
}

variable "enable_sqs_trigger" {
  description = "Enable SQS trigger for Lambda"
  type        = bool
  default     = true
}

variable "sqs_queue_arn" {
  description = "ARN of the SQS queue"
  type        = string
}

variable "sqs_batch_size" {
  description = "Batch size for SQS trigger"
  type        = number
  default     = 1
}

variable "max_concurrency" {
  description = "Maximum concurrent Lambda executions"
  type        = number
  default     = 10
}

variable "log_retention_days" {
  description = "CloudWatch log retention in days"
  type        = number
  default     = 7
}

variable "enable_error_alarm" {
  description = "Enable error alarm"
  type        = bool
  default     = true
}

variable "error_threshold" {
  description = "Error threshold for alarm"
  type        = number
  default     = 5
}

variable "enable_duration_alarm" {
  description = "Enable duration alarm"
  type        = bool
  default     = false
}

variable "duration_threshold" {
  description = "Duration threshold in milliseconds"
  type        = number
  default     = 270000 # 4.5 minutes
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}
