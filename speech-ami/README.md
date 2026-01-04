# Nestor's Speech AMI

This is an Amazon Machine Image (AMI) configured for Nestor Audio Server, which provides advanced speech recognition and natural language processing capabilities. The AMI includes all necessary dependencies and configurations to run the Nestor Audio Server seamlessly.

## How to build the AMI
This AMI is built using Packer. To build the AMI, follow these steps:
1. Install Packer on your local machine.
2. Clone the repository containing the Packer configuration files.
3. Navigate to the directory containing the Packer template for the Nestor Audio Server AMI.
4. Run the following command to build the AMI:
   ```bash
   packer build nestor-audio-server.json
   ```
5. Once the build is complete, note the AMI ID from the output.
