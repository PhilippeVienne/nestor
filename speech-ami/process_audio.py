#!/usr/bin/env python3
"""
Whisper Audio Processor - Processes audio files from SQS queue using OpenAI Whisper
"""
import boto3
import json
import os
import sys
import tempfile
import logging
from pathlib import Path
import whisper

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# Initialize AWS clients
sqs = boto3.client('sqs', region_name=os.environ.get('AWS_REGION', 'us-east-1'))
s3 = boto3.client('s3', region_name=os.environ.get('AWS_REGION', 'us-east-1'))

# Configuration from environment
QUEUE_URL = os.environ.get('SQS_QUEUE_URL')
AUDIO_BUCKET = os.environ.get('AUDIO_BUCKET_NAME')
TRANSCRIPT_BUCKET = os.environ.get('TRANSCRIPT_BUCKET_NAME')
WHISPER_MODEL = os.environ.get('WHISPER_MODEL', 'base')

# Load Whisper model
logger.info(f'Loading Whisper model: {WHISPER_MODEL}')
model = whisper.load_model(WHISPER_MODEL)
logger.info('Whisper model loaded successfully')


def download_audio(audio_key: str) -> str:
    """Download audio file from S3"""
    tmp_file = tempfile.NamedTemporaryFile(suffix='.audio', delete=False)
    tmp_path = tmp_file.name
    tmp_file.close()
    
    logger.info(f'Downloading {audio_key} from {AUDIO_BUCKET}')
    s3.download_file(AUDIO_BUCKET, audio_key, tmp_path)
    return tmp_path


def transcribe_audio(audio_path: str) -> dict:
    """Transcribe audio using Whisper"""
    logger.info(f'Transcribing audio: {audio_path}')
    result = model.transcribe(audio_path)
    return result


def upload_transcript(audio_key: str, transcript: dict):
    """Upload transcript to S3"""
    transcript_key = f"transcripts/{Path(audio_key).stem}.json"
    
    logger.info(f'Uploading transcript to {transcript_key}')
    s3.put_object(
        Bucket=TRANSCRIPT_BUCKET,
        Key=transcript_key,
        Body=json.dumps(transcript, indent=2),
        ContentType='application/json'
    )
    
    return transcript_key


def process_message(message: dict) -> bool:
    """Process a single SQS message"""
    try:
        body = json.loads(message['Body'])
        audio_key = body.get('audio_key')
        
        if not audio_key:
            logger.error('No audio_key in message body')
            return False
        
        logger.info(f'Processing audio file: {audio_key}')
        
        # Download audio
        audio_path = download_audio(audio_key)
        
        try:
            # Transcribe
            transcript = transcribe_audio(audio_path)
            
            # Upload transcript
            transcript_key = upload_transcript(audio_key, transcript)
            
            logger.info(f'Successfully processed {audio_key} -> {transcript_key}')
            return True
            
        finally:
            # Clean up temp file
            if os.path.exists(audio_path):
                os.unlink(audio_path)
                
    except Exception as e:
        logger.error(f'Error processing message: {str(e)}', exc_info=True)
        return False


def main():
    """Main processing loop"""
    if not QUEUE_URL or not AUDIO_BUCKET or not TRANSCRIPT_BUCKET:
        logger.error('Missing required environment variables')
        logger.error(f'SQS_QUEUE_URL: {QUEUE_URL}')
        logger.error(f'AUDIO_BUCKET_NAME: {AUDIO_BUCKET}')
        logger.error(f'TRANSCRIPT_BUCKET_NAME: {TRANSCRIPT_BUCKET}')
        sys.exit(1)
    
    logger.info('Starting Whisper audio processor')
    logger.info(f'Queue: {QUEUE_URL}')
    logger.info(f'Audio bucket: {AUDIO_BUCKET}')
    logger.info(f'Transcript bucket: {TRANSCRIPT_BUCKET}')
    
    while True:
        try:
            # Receive messages from SQS (long polling)
            response = sqs.receive_message(
                QueueUrl=QUEUE_URL,
                MaxNumberOfMessages=1,
                WaitTimeSeconds=20,
                VisibilityTimeout=900  # 15 minutes
            )
            
            messages = response.get('Messages', [])
            if not messages:
                logger.debug('No messages in queue')
                continue
            
            for message in messages:
                if process_message(message):
                    # Delete message on success
                    sqs.delete_message(
                        QueueUrl=QUEUE_URL,
                        ReceiptHandle=message['ReceiptHandle']
                    )
                    logger.info('Message processed and deleted from queue')
                else:
                    logger.error('Failed to process message, leaving in queue')
                    
        except KeyboardInterrupt:
            logger.info('Received shutdown signal, exiting gracefully')
            break
        except Exception as e:
            logger.error(f'Error in main loop: {str(e)}', exc_info=True)
            import time
            time.sleep(5)


if __name__ == '__main__':
    main()
