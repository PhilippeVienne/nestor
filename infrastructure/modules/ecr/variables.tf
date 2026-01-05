variable "repository_name" {
  description = "Name of the ECR repository"
  type        = string
}

variable "image_tag_mutability" {
  description = "The tag mutability setting for the repository (MUTABLE or IMMUTABLE)"
  type        = string
  default     = "MUTABLE"

  validation {
    condition     = contains(["MUTABLE", "IMMUTABLE"], var.image_tag_mutability)
    error_message = "image_tag_mutability must be either MUTABLE or IMMUTABLE."
  }
}

variable "scan_on_push" {
  description = "Indicates whether images are scanned after being pushed to the repository"
  type        = bool
  default     = true
}

variable "enable_kms_encryption" {
  description = "Enable KMS encryption for ECR"
  type        = bool
  default     = false
}

variable "kms_key_id" {
  description = "The ARN of the KMS key to use for encryption. If null, a new key will be created"
  type        = string
  default     = null
}

variable "kms_deletion_window_days" {
  description = "Duration in days after which the key is deleted after destruction of the resource"
  type        = number
  default     = 30

  validation {
    condition     = var.kms_deletion_window_days >= 7 && var.kms_deletion_window_days <= 30
    error_message = "kms_deletion_window_days must be between 7 and 30 days."
  }
}

variable "lifecycle_policy" {
  description = "ECR lifecycle policy configuration"
  type = object({
    rules = list(object({
      rulePriority = number
      description  = string
      selection = object({
        tagStatus     = string
        tagPrefixList = optional(list(string))
        countType     = string
        countUnit     = optional(string)
        countNumber   = number
      })
      action = object({
        type = string
      })
    }))
  })
}

variable "create_iam_policies" {
  description = "Whether to create IAM policies for ECR access"
  type        = bool
  default     = true
}

variable "tags" {
  description = "A map of tags to add to all resources"
  type        = map(string)
  default     = {}
}
