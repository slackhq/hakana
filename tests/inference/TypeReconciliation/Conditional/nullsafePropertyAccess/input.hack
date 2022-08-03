class IntLinkedList {
    public function __construct(
        public int $value,
        public ?IntLinkedList $next
    ) {}
}

function skipOne(IntLinkedList $l) : ?int {
    return $l->next?->value;
}

function skipTwo(IntLinkedList $l) : ?int {
    return $l->next?->next?->value;
}