variable "table_name" {
  description = "Name of the DynamoDB table"
  type        = string
}

variable "billing_mode" {
  description = "Billing mode for the table (PROVISIONED or PAY_PER_REQUEST)"
  type        = string
  default     = "PAY_PER_REQUEST"
  validation {
    condition     = contains(["PROVISIONED", "PAY_PER_REQUEST"], var.billing_mode)
    error_message = "Billing mode must be either PROVISIONED or PAY_PER_REQUEST."
  }
}

variable "hash_key" {
  description = "Hash key for the table"
  type        = string
}

variable "range_key" {
  description = "Range key for the table"
  type        = string
  default     = null
}

variable "attributes" {
  description = "List of attributes for the table"
  type = list(object({
    name = string
    type = string
  }))
}

variable "global_secondary_indexes" {
  description = "List of global secondary indexes"
  type        = list(any)
  default     = []
}

variable "read_capacity" {
  description = "Read capacity units (only for PROVISIONED billing mode)"
  type        = number
  default     = 5
}

variable "write_capacity" {
  description = "Write capacity units (only for PROVISIONED billing mode)"
  type        = number
  default     = 5
}

variable "ttl_enabled" {
  description = "Enable TTL for the table"
  type        = bool
  default     = false
}

variable "ttl_attribute_name" {
  description = "Name of the TTL attribute"
  type        = string
  default     = "ttl"
}

variable "point_in_time_recovery_enabled" {
  description = "Enable point-in-time recovery"
  type        = bool
  default     = true
}

variable "enable_encryption" {
  description = "Enable server-side encryption"
  type        = bool
  default     = true
}

variable "kms_key_arn" {
  description = "ARN of the KMS key for encryption (null for AWS managed key)"
  type        = string
  default     = null
}

variable "stream_enabled" {
  description = "Enable DynamoDB streams"
  type        = bool
  default     = false
}

variable "stream_view_type" {
  description = "Stream view type (KEYS_ONLY, NEW_IMAGE, OLD_IMAGE, NEW_AND_OLD_IMAGES)"
  type        = string
  default     = "NEW_AND_OLD_IMAGES"
}

variable "enable_autoscaling" {
  description = "Enable auto-scaling for provisioned tables"
  type        = bool
  default     = false
}

variable "autoscaling_read_min_capacity" {
  description = "Minimum read capacity for auto-scaling"
  type        = number
  default     = 1
}

variable "autoscaling_read_max_capacity" {
  description = "Maximum read capacity for auto-scaling"
  type        = number
  default     = 10
}

variable "autoscaling_read_target_value" {
  description = "Target utilization for read auto-scaling"
  type        = number
  default     = 70
}

variable "autoscaling_write_min_capacity" {
  description = "Minimum write capacity for auto-scaling"
  type        = number
  default     = 1
}

variable "autoscaling_write_max_capacity" {
  description = "Maximum write capacity for auto-scaling"
  type        = number
  default     = 10
}

variable "autoscaling_write_target_value" {
  description = "Target utilization for write auto-scaling"
  type        = number
  default     = 70
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}
