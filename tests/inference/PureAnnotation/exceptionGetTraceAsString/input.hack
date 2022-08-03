function getTraceAsString(Throwable $e)[]: string {
    return $e->getTraceAsString();
}

echo getTraceAsString(new Exception("test"));