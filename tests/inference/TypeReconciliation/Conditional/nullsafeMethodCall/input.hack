final class IntLinkedList {
    public function __construct(
        public int $value,
        private ?IntLinkedList $next
    ) {}

    public function getNext() : ?IntLinkedList {
        return $this->next;
    }

    public function bar(): string {
        return 'a';
    }
}

function skipOne(IntLinkedList $l) : ?int {
    return $l->getNext()?->value;
}

function skipTwo(IntLinkedList $l) : ?int {
    return $l->getNext()?->getNext()?->value;
}

function bar(?IntLinkedList $l) : ?string {
    return $l?->bar();
}
