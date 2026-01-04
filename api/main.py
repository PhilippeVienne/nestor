from fastapi import FastAPI, HTTPException, BackgroundTasks
from fastapi.responses import JSONResponse
from mangum import Mangum
from pydantic import BaseModel, Field
from typing import Optional, Dict, Any
import boto3
import json
import logging
import os
from datetime import datetime

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = FastAPI(
    title="Nestor API",
    description="FastAPI Lambda with SQS integration",
    version="1.0.0"
)

sqs_client = boto3.client('sqs', region_name=os.getenv('AWS_REGION', 'us-east-1'))
QUEUE_URL = os.getenv('SQS_QUEUE_URL', '')


class TaskRequest(BaseModel):
    task_type: str = Field(..., description="Type of task to process")
    payload: Dict[str, Any] = Field(..., description="Task payload data")
    priority: Optional[int] = Field(default=0, description="Task priority (0-10)")


class TaskResponse(BaseModel):
    message_id: str
    status: str
    timestamp: str


class HealthResponse(BaseModel):
    status: str
    timestamp: str
    environment: str


async def send_to_sqs(task: TaskRequest) -> str:
    """Send task to SQS queue"""
    try:
        message_body = {
            "task_type": task.task_type,
            "payload": task.payload,
            "priority": task.priority,
            "timestamp": datetime.utcnow().isoformat()
        }
        
        response = sqs_client.send_message(
            QueueUrl=QUEUE_URL,
            MessageBody=json.dumps(message_body),
            MessageAttributes={
                'TaskType': {
                    'StringValue': task.task_type,
                    'DataType': 'String'
                },
                'Priority': {
                    'StringValue': str(task.priority),
                    'DataType': 'Number'
                }
            }
        )
        
        logger.info(f"Message sent to SQS: {response['MessageId']}")
        return response['MessageId']
    
    except Exception as e:
        logger.error(f"Error sending message to SQS: {str(e)}")
        raise HTTPException(status_code=500, detail=f"Failed to queue task: {str(e)}")


@app.get("/", response_model=HealthResponse)
async def root():
    """Health check endpoint"""
    return HealthResponse(
        status="healthy",
        timestamp=datetime.utcnow().isoformat(),
        environment=os.getenv('ENVIRONMENT', 'development')
    )


@app.get("/health", response_model=HealthResponse)
async def health_check():
    """Detailed health check"""
    queue_configured = bool(QUEUE_URL)
    
    return HealthResponse(
        status="healthy" if queue_configured else "degraded",
        timestamp=datetime.utcnow().isoformat(),
        environment=os.getenv('ENVIRONMENT', 'development')
    )


@app.post("/tasks", response_model=TaskResponse, status_code=202)
async def create_task(task: TaskRequest):
    """
    Create a new task and send it to SQS queue for async processing
    """
    if not QUEUE_URL:
        raise HTTPException(
            status_code=503,
            detail="SQS queue not configured"
        )
    
    message_id = await send_to_sqs(task)
    
    return TaskResponse(
        message_id=message_id,
        status="queued",
        timestamp=datetime.utcnow().isoformat()
    )


@app.get("/tasks/{message_id}")
async def get_task_status(message_id: str):
    """
    Get task status by message ID
    Note: This is a placeholder. In production, you'd query DynamoDB or similar
    """
    return {
        "message_id": message_id,
        "status": "processing",
        "message": "Task status tracking requires DynamoDB integration"
    }


@app.post("/tasks/batch", status_code=202)
async def create_batch_tasks(tasks: list[TaskRequest]):
    """
    Create multiple tasks in batch
    """
    if not QUEUE_URL:
        raise HTTPException(
            status_code=503,
            detail="SQS queue not configured"
        )
    
    if len(tasks) > 10:
        raise HTTPException(
            status_code=400,
            detail="Maximum 10 tasks per batch"
        )
    
    entries = []
    for idx, task in enumerate(tasks):
        message_body = {
            "task_type": task.task_type,
            "payload": task.payload,
            "priority": task.priority,
            "timestamp": datetime.utcnow().isoformat()
        }
        
        entries.append({
            'Id': str(idx),
            'MessageBody': json.dumps(message_body),
            'MessageAttributes': {
                'TaskType': {
                    'StringValue': task.task_type,
                    'DataType': 'String'
                },
                'Priority': {
                    'StringValue': str(task.priority),
                    'DataType': 'Number'
                }
            }
        })
    
    try:
        response = sqs_client.send_message_batch(
            QueueUrl=QUEUE_URL,
            Entries=entries
        )
        
        return {
            "successful": len(response.get('Successful', [])),
            "failed": len(response.get('Failed', [])),
            "timestamp": datetime.utcnow().isoformat()
        }
    
    except Exception as e:
        logger.error(f"Error sending batch to SQS: {str(e)}")
        raise HTTPException(status_code=500, detail=f"Failed to queue tasks: {str(e)}")


# Lambda handler
handler = Mangum(app, lifespan="off")
