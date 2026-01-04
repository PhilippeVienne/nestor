# Lambda function for Bedrock NLP processing
resource "aws_lambda_function" "bedrock_processor" {
  filename         = var.lambda_zip_path
  function_name    = var.function_name
  role            = aws_iam_role.lambda.arn
  handler         = var.handler
  runtime         = var.runtime
  timeout         = var.timeout
  memory_size     = var.memory_size

  source_code_hash = var.lambda_zip_path != null ? filebase64sha256(var.lambda_zip_path) : null

  environment {
    variables = merge({
      TRANSCRIPT_BUCKET = var.transcript_bucket_name
      RDS_SECRET_ARN    = var.rds_secret_arn
      BEDROCK_MODEL_ID  = var.bedrock_model_id
      AWS_REGION        = var.aws_region
    }, var.environment_variables)
  }

  vpc_config {
    subnet_ids         = var.subnet_ids
    security_group_ids = [aws_security_group.lambda.id]
  }

  tags = var.tags
}

# Security Group for Lambda
resource "aws_security_group" "lambda" {
  name_prefix = "${var.function_name}-"
  description = "Security group for Lambda function"
  vpc_id      = var.vpc_id

  tags = merge(var.tags, {
    Name = "${var.function_name}-sg"
  })

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_vpc_security_group_egress_rule" "lambda_all" {
  security_group_id = aws_security_group.lambda.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  description       = "Allow all outbound traffic"
}

# IAM Role for Lambda
resource "aws_iam_role" "lambda" {
  name_prefix = "${var.function_name}-"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "lambda.amazonaws.com"
      }
    }]
  })

  tags = var.tags
}

# Lambda policy for basic execution and VPC
resource "aws_iam_role_policy_attachment" "lambda_vpc_execution" {
  role       = aws_iam_role.lambda.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

# Custom policy for accessing AWS services
resource "aws_iam_role_policy" "lambda_custom" {
  name_prefix = "${var.function_name}-"
  role        = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:PutObject"
        ]
        Resource = "${var.transcript_bucket_arn}/*"
      },
      {
        Effect = "Allow"
        Action = [
          "secretsmanager:GetSecretValue"
        ]
        Resource = var.rds_secret_arn
      },
      {
        Effect = "Allow"
        Action = [
          "bedrock:InvokeModel",
          "bedrock:InvokeModelWithResponseStream"
        ]
        Resource = "arn:aws:bedrock:${var.aws_region}::foundation-model/${var.bedrock_model_id}"
      },
      {
        Effect = "Allow"
        Action = [
          "sqs:ReceiveMessage",
          "sqs:DeleteMessage",
          "sqs:GetQueueAttributes"
        ]
        Resource = var.sqs_queue_arn
      },
      {
        Effect = "Allow"
        Action = [
          "logs:CreateLogGroup",
          "logs:CreateLogStream",
          "logs:PutLogEvents"
        ]
        Resource = "arn:aws:logs:*:*:*"
      }
    ]
  })
}

# SQS trigger for Lambda
resource "aws_lambda_event_source_mapping" "sqs" {
  count = var.enable_sqs_trigger ? 1 : 0

  event_source_arn = var.sqs_queue_arn
  function_name    = aws_lambda_function.bedrock_processor.arn
  batch_size       = var.sqs_batch_size
  enabled          = true

  scaling_config {
    maximum_concurrency = var.max_concurrency
  }

  function_response_types = ["ReportBatchItemFailures"]
}

# CloudWatch Log Group
resource "aws_cloudwatch_log_group" "lambda" {
  name              = "/aws/lambda/${var.function_name}"
  retention_in_days = var.log_retention_days

  tags = var.tags
}

# CloudWatch Alarms
resource "aws_cloudwatch_metric_alarm" "lambda_errors" {
  count = var.enable_error_alarm ? 1 : 0

  alarm_name          = "${var.function_name}-errors"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "1"
  metric_name         = "Errors"
  namespace           = "AWS/Lambda"
  period              = "300"
  statistic           = "Sum"
  threshold           = var.error_threshold
  alarm_description   = "Lambda function error rate"
  treat_missing_data  = "notBreaching"

  dimensions = {
    FunctionName = aws_lambda_function.bedrock_processor.function_name
  }

  tags = var.tags
}

resource "aws_cloudwatch_metric_alarm" "lambda_duration" {
  count = var.enable_duration_alarm ? 1 : 0

  alarm_name          = "${var.function_name}-duration"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "1"
  metric_name         = "Duration"
  namespace           = "AWS/Lambda"
  period              = "300"
  statistic           = "Average"
  threshold           = var.duration_threshold
  alarm_description   = "Lambda function duration"
  treat_missing_data  = "notBreaching"

  dimensions = {
    FunctionName = aws_lambda_function.bedrock_processor.function_name
  }

  tags = var.tags
}
