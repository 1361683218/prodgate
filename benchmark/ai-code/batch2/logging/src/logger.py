import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

def process_order(order):
    try:
        charge(order)
    except Exception as e:
        logger.error(f"Order failed: {e}")
        raise
