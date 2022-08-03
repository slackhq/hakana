final class A
{
    private ?string $property = null;

    public function handle(string $arg): string
    {
        if (null !== $this->property) {
            return $arg;
        }

        try {
            return $arg;
        } finally {
        }
    }
}