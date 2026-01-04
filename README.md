# Nestor Audio Server

Nestor Audio Server is an assistant that receives audio files from users, processes them using advanced speech recognition and natural language processing techniques. It then stores an organized record of the interactions in a database for future reference. It provides a chat bot with the user's audio history and context to enhance the conversation experience.

Each conversation is stored in a PostgreSQL database, allowing for easy retrieval and analysis of past interactions. The server is built using FastAPI for handling requests and SQLAlchemy for database interactions.

## Features
- Receive and process audio files from users
- Transcribe audio to text using speech recognition
- Generate responses using natural language processing
- Store conversations in a PostgreSQL database
- Provide a chat bot interface with access to user's audio history
- Built with FastAPI and SQLAlchemy for robust performance
- Frontend app built with React and TypeScript and distributed via CloudFront and S3

## Architecture
The Nestor Audio Server is structured as follows:
- **FastAPI**: Handles incoming HTTP requests and routes them to the appropriate endpoints.
- **SQLAlchemy**: Manages database interactions and ORM mapping for PostgreSQL.
- **Speech Recognition**: Converts audio files to text. (using OpenAI Whisper)
- **Natural Language Processing**: Generates responses based on transcribed text and conversation history. (using Bedrock)
- **PostgreSQL**: Stores user conversations and audio history.

Infrastructure is designed to be deployed on AWS using services like API Gateway, Lambda, and RDS for scalability and reliability.

Speech recognition is performed using OpenAI Whisper in an EC2 ASG, with a SQS queue to manage incoming audio processing requests.
Natural language processing is handled using AWS Bedrock from Lambda functions and SQS queues.

## Installation
1. From the infrastructure directory, deploy the Terraform scripts to set up the necessary AWS resources.
2. Use 