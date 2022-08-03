function getFile(Throwable $e)[]: string {
    return $e->getFile();
}

echo getFile(new Exception("test"));