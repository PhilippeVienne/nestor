from pydantic_settings import BaseSettings
from typing import Optional


class Settings(BaseSettings):
    """Application settings"""
    
    # AWS Configuration
    aws_region: str = "us-east-1"
    sqs_queue_url: Optional[str] = None
    dynamodb_table: Optional[str] = None
    
    # Application Configuration
    environment: str = "development"
    log_level: str = "INFO"
    
    # API Configuration
    api_title: str = "Nestor API"
    api_version: str = "1.0.0"
    
    class Config:
        env_file = ".env"
        case_sensitive = False


settings = Settings()
