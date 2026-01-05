# Security Group for EC2 instances
resource "aws_security_group" "whisper" {
  name_prefix = "${var.name_prefix}-whisper-"
  description = "Security group for Whisper EC2 instances"
  vpc_id      = var.vpc_id

  tags = merge(var.tags, {
    Name = "${var.name_prefix}-whisper-sg"
  })

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_vpc_security_group_ingress_rule" "whisper_ssh" {
  count = var.enable_ssh_access ? 1 : 0

  security_group_id = aws_security_group.whisper.id
  cidr_ipv4         = var.ssh_cidr_block
  from_port         = 22
  to_port           = 22
  ip_protocol       = "tcp"
  description       = "SSH access"
}

resource "aws_vpc_security_group_egress_rule" "whisper_all" {
  security_group_id = aws_security_group.whisper.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  description       = "Allow all outbound traffic"
}

# IAM Role for EC2 instances
resource "aws_iam_role" "whisper" {
  name_prefix = "${var.name_prefix}-whisper-"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
    }]
  })

  tags = var.tags
}

# Attach policies for S3, SQS, and CloudWatch
resource "aws_iam_role_policy" "whisper" {
  name_prefix = "${var.name_prefix}-whisper-"
  role        = aws_iam_role.whisper.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:PutObject",
          "s3:DeleteObject"
        ]
        Resource = [
          "${var.audio_bucket_arn}/*",
          "${var.transcript_bucket_arn}/*"
        ]
      },
      {
        Effect = "Allow"
        Action = [
          "s3:ListBucket"
        ]
        Resource = [
          var.audio_bucket_arn,
          var.transcript_bucket_arn
        ]
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
      },
      {
        Effect = "Allow"
        Action = [
          "secretsmanager:GetSecretValue"
        ]
        Resource = "*"
      }
    ]
  })
}

resource "aws_iam_instance_profile" "whisper" {
  name_prefix = "${var.name_prefix}-whisper-"
  role        = aws_iam_role.whisper.name

  tags = var.tags
}

# Launch Template
resource "aws_launch_template" "whisper" {
  name_prefix   = "${var.name_prefix}-whisper-"
  image_id      = var.ami_id
  instance_type = var.instance_type

  iam_instance_profile {
    arn = aws_iam_instance_profile.whisper.arn
  }

  vpc_security_group_ids = [aws_security_group.whisper.id]

  user_data = base64encode(templatefile("${path.module}/user_data.sh", {
    sqs_queue_url          = var.sqs_queue_url
    audio_bucket_name      = var.audio_bucket_name
    transcript_bucket_name = var.transcript_bucket_name
    aws_region             = var.aws_region
  }))

  block_device_mappings {
    device_name = "/dev/sda1"
    ebs {
      volume_size           = var.root_volume_size
      volume_type           = var.root_volume_type
      delete_on_termination = true
      encrypted             = true
    }
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
  }

  monitoring {
    enabled = var.enable_detailed_monitoring
  }

  tag_specifications {
    resource_type = "instance"
    tags = merge(var.tags, {
      Name = "${var.name_prefix}-whisper"
    })
  }

  tag_specifications {
    resource_type = "volume"
    tags = merge(var.tags, {
      Name = "${var.name_prefix}-whisper-volume"
    })
  }

  lifecycle {
    create_before_destroy = true
  }

  tags = var.tags
}

# Auto Scaling Group
resource "aws_autoscaling_group" "whisper" {
  name_prefix               = "${var.name_prefix}-whisper-"
  vpc_zone_identifier       = var.subnet_ids
  min_size                  = var.asg_min_size
  max_size                  = var.asg_max_size
  desired_capacity          = var.asg_desired_capacity
  health_check_type         = "EC2"
  health_check_grace_period = 300

  launch_template {
    id      = aws_launch_template.whisper.id
    version = "$Latest"
  }

  dynamic "tag" {
    for_each = var.tags
    content {
      key                 = tag.key
      value               = tag.value
      propagate_at_launch = true
    }
  }

  tag {
    key                 = "Name"
    value               = "${var.name_prefix}-whisper-asg"
    propagate_at_launch = false
  }

  lifecycle {
    create_before_destroy = true
  }
}

# Auto Scaling Policy - Scale up based on SQS queue depth
resource "aws_autoscaling_policy" "scale_up" {
  name                   = "${var.name_prefix}-whisper-scale-up"
  scaling_adjustment     = 1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 300
  autoscaling_group_name = aws_autoscaling_group.whisper.name
}

resource "aws_cloudwatch_metric_alarm" "queue_depth_high" {
  alarm_name          = "${var.name_prefix}-whisper-queue-depth-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "2"
  metric_name         = "ApproximateNumberOfMessagesVisible"
  namespace           = "AWS/SQS"
  period              = "60"
  statistic           = "Average"
  threshold           = var.scale_up_threshold
  alarm_description   = "Scale up when queue depth is high"
  alarm_actions       = [aws_autoscaling_policy.scale_up.arn]

  dimensions = {
    QueueName = var.sqs_queue_name
  }

  tags = var.tags
}

# Auto Scaling Policy - Scale down
resource "aws_autoscaling_policy" "scale_down" {
  name                   = "${var.name_prefix}-whisper-scale-down"
  scaling_adjustment     = -1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 300
  autoscaling_group_name = aws_autoscaling_group.whisper.name
}

resource "aws_cloudwatch_metric_alarm" "queue_depth_low" {
  alarm_name          = "${var.name_prefix}-whisper-queue-depth-low"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = "2"
  metric_name         = "ApproximateNumberOfMessagesVisible"
  namespace           = "AWS/SQS"
  period              = "60"
  statistic           = "Average"
  threshold           = var.scale_down_threshold
  alarm_description   = "Scale down when queue depth is low"
  alarm_actions       = [aws_autoscaling_policy.scale_down.arn]

  dimensions = {
    QueueName = var.sqs_queue_name
  }

  tags = var.tags
}
