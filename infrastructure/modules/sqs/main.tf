resource "aws_sqs_queue" "main" {
  name                       = var.queue_name
  delay_seconds              = var.delay_seconds
  max_message_size           = var.max_message_size
  message_retention_seconds  = var.message_retention_seconds
  receive_wait_time_seconds  = var.receive_wait_time_seconds
  visibility_timeout_seconds = var.visibility_timeout_seconds

  # Dead Letter Queue configuration
  redrive_policy = var.enable_dlq ? jsonencode({
    deadLetterTargetArn = aws_sqs_queue.dlq[0].arn
    maxReceiveCount     = var.max_receive_count
  }) : null

  # Server-side encryption
  sqs_managed_sse_enabled = var.enable_sse

  tags = var.tags
}

# Dead Letter Queue
resource "aws_sqs_queue" "dlq" {
  count = var.enable_dlq ? 1 : 0

  name                       = "${var.queue_name}-dlq"
  message_retention_seconds  = var.dlq_message_retention_seconds
  sqs_managed_sse_enabled    = var.enable_sse

  tags = merge(var.tags, {
    Purpose = "DeadLetterQueue"
  })
}

# CloudWatch Alarms for DLQ
resource "aws_cloudwatch_metric_alarm" "dlq_messages" {
  count = var.enable_dlq && var.enable_dlq_alarm ? 1 : 0

  alarm_name          = "${var.queue_name}-dlq-messages"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "1"
  metric_name         = "ApproximateNumberOfMessagesVisible"
  namespace           = "AWS/SQS"
  period              = "300"
  statistic           = "Average"
  threshold           = var.dlq_alarm_threshold
  alarm_description   = "This metric monitors messages in DLQ"
  treat_missing_data  = "notBreaching"

  dimensions = {
    QueueName = aws_sqs_queue.dlq[0].name
  }

  tags = var.tags
}
