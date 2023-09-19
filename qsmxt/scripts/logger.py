import os
import logging as _logging
from enum import Enum

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

def make_logger(name='main', logpath=None, printlevel=LogLevel.INFO, warnlevel=LogLevel.WARNING, errorlevel=LogLevel.ERROR, writelevel=LogLevel.INFO):
    # create/get logger
    logger = _logging.getLogger(name=name)

    # check level names if needed
    for log_level in LogLevel:
        if log_level.value not in _logging._levelToName.values():
            _logging.addLevelName(log_level.value, log_level.name)

    # return logger if it already has 3 handlers and we aren't writing to file
    if logger.hasHandlers() and (len(logger.handlers) == 3 and logpath is None):
        return logger

    # return logger with updated baseFilename path if necessary if there are 4 handlers
    if logpath: logpath = os.path.abspath(logpath)
    if logger.hasHandlers() and len(logger.handlers) == 4:
        logger.handlers[3].baseFilename == logpath
        return logger

    # create handlers
    console_handler = _logging.StreamHandler(stream=_StringStream())
    warnings_handler = _logging.StreamHandler(stream=_StringStream(print_new_records=False))
    errors_handler = _logging.StreamHandler(stream=_StringStream(print_new_records=False))
    if logpath:
        file_handler = _logging.FileHandler(logpath, mode='w')

    # create formatter
    # formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
    # https://docs.python.org/3.7/library/logging.html#logrecord-attributes
    formatter = _logging.Formatter('[%(levelname)s]: %(message)s')

    # add formatters to handlers
    console_handler.setFormatter(formatter)
    warnings_handler.setFormatter(formatter)
    errors_handler.setFormatter(formatter)
    if logpath:
        file_handler.setFormatter(formatter)

    # add handlers to logger
    if logger.hasHandlers() == False or len(logger.handlers) == 0:
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
    warnings_occurred = False
    errors_occurred = False
    
    if len(logger.handlers) > 1:
        for message in logger.handlers[1].stream.items:
            if "WARNING" in message:
                warnings_occurred = True
                break
    
    if len(logger.handlers) > 2:
        for message in logger.handlers[2].stream.items:
            if "ERROR" in message:
                errors_occurred = True
                break
                
    if warnings_occurred:
        logger.log(LogLevel.INFO.value, "Warnings occurred!")
        
    if errors_occurred:
        logger.log(LogLevel.INFO.value, "Errors occurred!")

