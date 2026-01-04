#!/bin/bash
set -e

# Configure AWS region
export AWS_DEFAULT_REGION=${aws_region}
export AWS_REGION=${aws_region}

# Start CloudWatch agent
/opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl \
    -a fetch-config \
    -m ec2 \
    -s \
    -c default

# Run Whisper processor Docker container
docker run -d \
    --name whisper-processor \
    --restart unless-stopped \
    -e AWS_REGION=${aws_region} \
    -e SQS_QUEUE_URL=${sqs_queue_url} \
    -e AUDIO_BUCKET_NAME=${audio_bucket_name} \
    -e TRANSCRIPT_BUCKET_NAME=${transcript_bucket_name} \
    -e WHISPER_MODEL=base \
    --log-driver=awslogs \
    --log-opt awslogs-region=${aws_region} \
    --log-opt awslogs-group=/aws/ec2/whisper-processor \
    --log-opt awslogs-create-group=true \
    nestor-whisper-processor:latest

logger "Whisper processor started successfully"

