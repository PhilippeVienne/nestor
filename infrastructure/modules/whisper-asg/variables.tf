variable "name_prefix" {
  description = "Prefix for resource names"
  type        = string
}

variable "vpc_id" {
  description = "VPC ID"
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for the ASG"
  type        = list(string)
}

variable "ami_id" {
  description = "AMI ID for EC2 instances"
  type        = string
}

variable "instance_type" {
  description = "EC2 instance type"
  type        = string
  default     = "t3.medium"
}

variable "root_volume_size" {
  description = "Size of root volume in GB"
  type        = number
  default     = 30
}

variable "root_volume_type" {
  description = "Type of root volume"
  type        = string
  default     = "gp3"
}

variable "enable_detailed_monitoring" {
  description = "Enable detailed CloudWatch monitoring"
  type        = bool
  default     = true
}

variable "enable_ssh_access" {
  description = "Enable SSH access"
  type        = bool
  default     = false
}

variable "ssh_cidr_block" {
  description = "CIDR block for SSH access"
  type        = string
  default     = "0.0.0.0/0"
}

variable "asg_min_size" {
  description = "Minimum size of ASG"
  type        = number
  default     = 1
}

variable "asg_max_size" {
  description = "Maximum size of ASG"
  type        = number
  default     = 5
}

variable "asg_desired_capacity" {
  description = "Desired capacity of ASG"
  type        = number
  default     = 1
}

variable "sqs_queue_url" {
  description = "URL of the SQS queue"
  type        = string
}

variable "sqs_queue_name" {
  description = "Name of the SQS queue"
  type        = string
}

variable "sqs_queue_arn" {
  description = "ARN of the SQS queue"
  type        = string
}

variable "audio_bucket_name" {
  description = "Name of the audio recordings bucket"
  type        = string
}

variable "audio_bucket_arn" {
  description = "ARN of the audio recordings bucket"
  type        = string
}

variable "transcript_bucket_name" {
  description = "Name of the transcripts bucket"
  type        = string
}

variable "transcript_bucket_arn" {
  description = "ARN of the transcripts bucket"
  type        = string
}

variable "aws_region" {
  description = "AWS region"
  type        = string
}

variable "scale_up_threshold" {
  description = "SQS queue depth threshold to trigger scale up"
  type        = number
  default     = 5
}

variable "scale_down_threshold" {
  description = "SQS queue depth threshold to trigger scale down"
  type        = number
  default     = 1
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}
