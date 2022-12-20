import logging as _logging
from nipype import logging as _np_logging
import json as _json
from enum import Enum
from nipype.pipeline.engine import MapNode

class LogLevel(Enum):
    CRITICAL = 50
    ERROR = 40
    WARNING = 30
    INFO = 20
    DEBUG = 10
    NOTSET = 0

class _StringStream:
    '''
    a stream used to keep track of messages in a list
    '''

    def __init__(self, max_records=None, print_new_records=True):
        self.items = []
        self.max_records = max_records
        self.print_new_records = print_new_records

    def __get__(self, index):
        return self.items[index]

    def __len__(self):
        return len(self.items)

    def write(self, record):
        self.items.append(record)

        if self.print_new_records:
            print(record, end="")

        if self.max_records and len(self.items) > self.max_records:
            self.items.pop(0)

    def flush(self):
        pass

def get_logger():
    return _logging.getLogger(name='main')

def make_logger(logpath=None, printlevel=LogLevel.INFO, warnlevel=LogLevel.WARNING, errorlevel=LogLevel.ERROR, writelevel=LogLevel.WARNING):
    
    for log_level in LogLevel:
        _logging.addLevelName(log_level.value, log_level.name)

    # create logger
    logger = _logging.getLogger(name='main')

    # create console handler and set level to my level
    console_handler = _logging.StreamHandler(stream=_StringStream())
    warnings_handler = _logging.StreamHandler(stream=_StringStream(print_new_records=False))
    errors_handler = _logging.StreamHandler(stream=_StringStream(print_new_records=False))
    if logpath:
        file_handler = _logging.FileHandler(logpath, mode='w')

    # create formatter
    # formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
    # https://docs.python.org/3.7/library/logging.html#logrecord-attributes
    formatter = _logging.Formatter('%(levelname)s: %(message)s')

    # add formatters to handlers
    console_handler.setFormatter(formatter)
    warnings_handler.setFormatter(formatter)
    errors_handler.setFormatter(formatter)
    if logpath:
        file_handler.setFormatter(formatter)

    # add handlers to logger
    logger.addHandler(console_handler)
    logger.addHandler(warnings_handler)
    logger.addHandler(errors_handler)
    if logpath:
        logger.addHandler(file_handler)

    # set log levels
    logger.handlers[0].setLevel(printlevel.value)
    logger.handlers[1].setLevel(warnlevel.value)
    logger.handlers[2].setLevel(errorlevel.value)
    if logpath:
        logger.handlers[3].setLevel(writelevel.value)
    logger.setLevel(printlevel.value)

    return logger


def show_log(logger):
    for message in logger.handlers[0].stream.items:
        print(message, end='')

def show_warning_summary(logger):
    if logger.handlers[1].stream.items:
        logger.log(LogLevel.INFO.value, "Process completed with warnings. Some runs may have been skipped.")
    if logger.handlers[2].stream.items:
        logger.log(LogLevel.INFO.value, "Errors occurred - outputs may not be usable.")

def log_nodes_cb(node, status):
    """Function to record node run statistics to a log file as json
    dictionaries

    Parameters
    ----------
    node : nipype.pipeline.engine.Node
        the node being logged
    status : string
        acceptable values are 'start', 'end'; otherwise it is
        considered and error

    Returns
    -------
    None
        this function does not return any values, it logs the node
        status info to the callback logger
    """
    if status != "end":
        return
    if isinstance(node, MapNode):
        return

    status_dict = {
        "name": node.name,
        "id": node._id,
        "start": getattr(node.result.runtime, "startTime"),
        "finish": getattr(node.result.runtime, "endTime"),
        "duration": getattr(node.result.runtime, "duration"),
        "runtime_threads": getattr(node.result.runtime, "cpu_percent", "N/A"),
        "runtime_memory_gb": getattr(node.result.runtime, "mem_peak_gb", "N/A"),
        "estimated_memory_gb": node.mem_gb,
        "num_threads": node.n_procs,
    }

    if status_dict["start"] is None or status_dict["finish"] is None:
        status_dict["error"] = True

    # Dump string to log
    _np_logging.getLogger("callback").debug(_json.dumps(status_dict))

