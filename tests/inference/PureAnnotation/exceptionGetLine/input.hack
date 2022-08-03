function getLine(Throwable $e)[]: int {
    return $e->getLine();
}

echo getLine(new Exception("test"));