FROM public.ecr.aws/lambda/python:3.11

# Copy requirements
COPY requirements.txt ${LAMBDA_TASK_ROOT}/

# Install dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY main.py ${LAMBDA_TASK_ROOT}/
COPY sqs_processor.py ${LAMBDA_TASK_ROOT}/
COPY config.py ${LAMBDA_TASK_ROOT}/

# Set the CMD to your handler
CMD [ "main.handler" ]
