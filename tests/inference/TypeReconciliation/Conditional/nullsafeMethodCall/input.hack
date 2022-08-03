class IntLinkedList {
    public function __construct(
        public int $value,
        private ?IntLinkedList $next
    ) {}

    public function getNext() : ?IntLinkedList {
        return $this->next;
    }
}

function skipOne(IntLinkedList $l) : ?int {
    return $l->getNext()?->value;
}

function skipTwo(IntLinkedList $l) : ?int {
    return $l->getNext()?->getNext()?->value;
}