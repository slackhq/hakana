class TestMe {
    private function startTransaction(): void {
    }

    private function endTransaction(bool $commit): void {
        echo $commit ? "Committing" : "Rolling back";
    }

    public function doWork(): void {
        $this->startTransaction();
        try {
            $this->workThatMayOrMayNotThrow();
            $success = true;
        } catch (Exception $e) {
        } finally {
            $this->endTransaction($success ?? false);
        }
    }

    private function workThatMayOrMayNotThrow(): void {}
}