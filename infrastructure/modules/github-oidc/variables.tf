variable "role_name_prefix" {
  description = "Prefix for IAM role name"
  type        = string
  default     = "github-actions-"
}

variable "github_repositories" {
  description = "List of GitHub repositories allowed to assume this role (format: owner/repo)"
  type        = list(string)
}

variable "custom_policies" {
  description = "Map of custom IAM policies to attach (name -> policy document)"
  type        = map(string)
  default     = {}
}

variable "managed_policy_arns" {
  description = "List of managed IAM policy ARNs to attach"
  type        = list(string)
  default     = []
}

variable "enable_default_policies" {
  description = "Enable default CI/CD policies"
  type        = bool
  default     = true
}

variable "enable_ecr_access" {
  description = "Enable ECR access in default policies"
  type        = bool
  default     = true
}

variable "ecr_repository_arns" {
  description = "List of ECR repository ARNs to grant access"
  type        = list(string)
  default     = []
}

variable "enable_s3_access" {
  description = "Enable S3 access in default policies"
  type        = bool
  default     = false
}

variable "s3_bucket_arns" {
  description = "List of S3 bucket ARNs to grant access"
  type        = list(string)
  default     = []
}

variable "enable_lambda_deploy" {
  description = "Enable Lambda deployment permissions"
  type        = bool
  default     = false
}

variable "lambda_function_arns" {
  description = "List of Lambda function ARNs to grant deployment access"
  type        = list(string)
  default     = []
}

variable "enable_terraform_state" {
  description = "Enable Terraform state access"
  type        = bool
  default     = true
}

variable "terraform_state_bucket_arn" {
  description = "ARN of Terraform state bucket"
  type        = string
  default     = ""
}

variable "terraform_lock_table_arn" {
  description = "ARN of Terraform lock DynamoDB table"
  type        = string
  default     = null
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}
