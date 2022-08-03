function getMessage(Throwable $e)[]: string {
    return $e->getMessage();
}

echo getMessage(new Exception("test"));