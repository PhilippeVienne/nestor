# Nestor API

FastAPI application designed to run on AWS Lambda with API Gateway integration and SQS queue processing.

## Architecture

- **API Gateway** → **Lambda (FastAPI + Mangum)** → **SQS Queue**
- **SQS Queue** → **Lambda (Processor)** → **DynamoDB** (optional)

## Components

### 1. FastAPI Application (`main.py`)
- RESTful API endpoints
- Mangum adapter for Lambda integration
- Sends tasks to SQS queue for async processing

### 2. SQS Processor (`sqs_processor.py`)
- Lambda function triggered by SQS
- Processes tasks from the queue
- Stores results in DynamoDB (optional)

### 3. Configuration (`config.py`)
- Environment-based settings
- Uses pydantic-settings for validation

## Endpoints

### `GET /`
Health check endpoint

### `GET /health`
Detailed health status

### `POST /tasks`
Create a new task
```json
{
  "task_type": "speech_synthesis",
  "payload": {
    "text": "Hello world",
    "voice": "en-US-Neural2-A"
  },
  "priority": 5
}
```

### `POST /tasks/batch`
Create multiple tasks (max 10)

### `GET /tasks/{message_id}`
Get task status (requires DynamoDB integration)

## Local Development

### Install dependencies
```bash
pip install -r requirements.txt
```

### Run locally with uvicorn
```bash
uvicorn main:app --reload --port 8000
```

### Test the API
```bash
curl http://localhost:8000/health

curl -X POST http://localhost:8000/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "task_type": "speech_synthesis",
    "payload": {"text": "Hello world"},
    "priority": 5
  }'
```

## Docker Build

```bash
docker build -t nestor-api .
```

## AWS Deployment

1. Create SQS Queue
2. Create DynamoDB Table (optional)
3. Deploy Lambda function with Docker image
4. Configure API Gateway
5. Set environment variables:
   - `SQS_QUEUE_URL`
   - `DYNAMODB_TABLE`
   - `AWS_REGION`
   - `ENVIRONMENT`

## Environment Variables

- `AWS_REGION`: AWS region (default: us-east-1)
- `SQS_QUEUE_URL`: SQS queue URL for task processing
- `DYNAMODB_TABLE`: DynamoDB table for task results
- `ENVIRONMENT`: Environment name (development/staging/production)
- `LOG_LEVEL`: Logging level (INFO/DEBUG/WARNING/ERROR)

## Task Types

Extend `sqs_processor.py` to add custom task processing:

- `speech_synthesis`: Text-to-speech conversion
- `audio_processing`: Audio file processing
- `data_transform`: Data transformation tasks

Add your own task types by implementing new processor functions.
