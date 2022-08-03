class Test {
    private int $retryAttempts = 10;

    private function getResult(): string
    {
        // return tring or throw exception whatever
        throw new Exception();
    }

    private function getResultWithRetry(): string
    {
        $attempt = 1;

        while (true) {
            try {
                return $this->getResult();
            } catch (Throwable $exception) {
                if ($attempt >= $this->retryAttempts) {
throw $exception;
                }

                $attempt++;

                continue;
            }
        }
    }
}