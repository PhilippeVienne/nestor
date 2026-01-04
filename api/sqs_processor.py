import json
import logging
import os
import boto3
from typing import Dict, Any

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

sqs_client = boto3.client('sqs', region_name=os.getenv('AWS_REGION', 'us-east-1'))
dynamodb = boto3.resource('dynamodb', region_name=os.getenv('AWS_REGION', 'us-east-1'))


def process_task(task_type: str, payload: Dict[str, Any]) -> Dict[str, Any]:
    """
    Process different task types
    Add your business logic here
    """
    logger.info(f"Processing task type: {task_type}")
    
    if task_type == "speech_synthesis":
        return process_speech_synthesis(payload)
    elif task_type == "audio_processing":
        return process_audio(payload)
    elif task_type == "data_transform":
        return process_data_transform(payload)
    else:
        logger.warning(f"Unknown task type: {task_type}")
        return {"status": "error", "message": f"Unknown task type: {task_type}"}


def process_speech_synthesis(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Process speech synthesis task"""
    text = payload.get("text", "")
    voice = payload.get("voice", "default")
    
    logger.info(f"Synthesizing speech for text length: {len(text)}, voice: {voice}")
    
    # Add your speech synthesis logic here
    # This is a placeholder
    
    return {
        "status": "completed",
        "result": {
            "audio_url": "s3://bucket/path/to/audio.mp3",
            "duration": 10.5,
            "voice": voice
        }
    }


def process_audio(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Process audio task"""
    audio_url = payload.get("audio_url", "")
    operation = payload.get("operation", "")
    
    logger.info(f"Processing audio: {audio_url}, operation: {operation}")
    
    # Add your audio processing logic here
    
    return {
        "status": "completed",
        "result": {
            "processed_url": "s3://bucket/path/to/processed.mp3",
            "operation": operation
        }
    }


def process_data_transform(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Process data transformation task"""
    data = payload.get("data", {})
    transform_type = payload.get("transform_type", "")
    
    logger.info(f"Transforming data with type: {transform_type}")
    
    # Add your data transformation logic here
    
    return {
        "status": "completed",
        "result": {
            "transformed_data": data,
            "transform_type": transform_type
        }
    }


def save_task_result(message_id: str, result: Dict[str, Any]):
    """Save task result to DynamoDB"""
    table_name = os.getenv('DYNAMODB_TABLE', '')
    
    if not table_name:
        logger.warning("DynamoDB table not configured, skipping result storage")
        return
    
    try:
        table = dynamodb.Table(table_name)
        table.put_item(
            Item={
                'message_id': message_id,
                'result': json.dumps(result),
                'timestamp': result.get('timestamp', ''),
                'status': result.get('status', 'unknown')
            }
        )
        logger.info(f"Saved result for message: {message_id}")
    except Exception as e:
        logger.error(f"Error saving to DynamoDB: {str(e)}")


def handler(event, context):
    """
    Lambda handler for SQS trigger
    Processes messages from SQS queue
    """
    logger.info(f"Processing {len(event.get('Records', []))} SQS messages")
    
    results = []
    
    for record in event.get('Records', []):
        message_id = record['messageId']
        receipt_handle = record['receiptHandle']
        
        try:
            body = json.loads(record['body'])
            task_type = body.get('task_type')
            payload = body.get('payload', {})
            priority = body.get('priority', 0)
            
            logger.info(f"Processing message {message_id}: type={task_type}, priority={priority}")
            
            # Process the task
            result = process_task(task_type, payload)
            
            # Save result
            save_task_result(message_id, result)
            
            results.append({
                "messageId": message_id,
                "status": "success",
                "result": result
            })
            
        except Exception as e:
            logger.error(f"Error processing message {message_id}: {str(e)}")
            results.append({
                "messageId": message_id,
                "status": "error",
                "error": str(e)
            })
    
    return {
        "statusCode": 200,
        "body": json.dumps({
            "processed": len(results),
            "results": results
        })
    }
