variable "queue_name" {
  description = "Name of the SQS queue"
  type        = string
}

variable "delay_seconds" {
  description = "The time in seconds that the delivery of all messages in the queue will be delayed"
  type        = number
  default     = 0
}

variable "max_message_size" {
  description = "The limit of how many bytes a message can contain before Amazon SQS rejects it"
  type        = number
  default     = 262144 # 256 KB
}

variable "message_retention_seconds" {
  description = "The number of seconds Amazon SQS retains a message"
  type        = number
  default     = 345600 # 4 days
}

variable "receive_wait_time_seconds" {
  description = "The time for which a ReceiveMessage call will wait for a message to arrive"
  type        = number
  default     = 10
}

variable "visibility_timeout_seconds" {
  description = "The visibility timeout for the queue"
  type        = number
  default     = 300 # 5 minutes
}

variable "enable_dlq" {
  description = "Enable Dead Letter Queue"
  type        = bool
  default     = true
}

variable "max_receive_count" {
  description = "The number of times a message is delivered before being moved to DLQ"
  type        = number
  default     = 3
}

variable "dlq_message_retention_seconds" {
  description = "The number of seconds Amazon SQS retains a message in DLQ"
  type        = number
  default     = 1209600 # 14 days
}

variable "enable_sse" {
  description = "Enable server-side encryption"
  type        = bool
  default     = true
}

variable "enable_dlq_alarm" {
  description = "Enable CloudWatch alarm for DLQ"
  type        = bool
  default     = true
}

variable "dlq_alarm_threshold" {
  description = "Threshold for DLQ alarm"
  type        = number
  default     = 1
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}
