final class Bar {
    const ENABLE_STALE_ATTENDEE_DELETION = true;
    const ENABLE_STALE_ATTENDEE_DELETION_ON_REJOIN = false;

    public static function foo(EventType $event_type, vec<string> $a, vec<string> $b): void {
        if ($event_type === EventType::CALL_ENDED) {
            return;
        }

        if (
            $event_type !== EventType::CALL_PARTICIPANT_JOINED &&
            $event_type !== EventType::CALL_PARTICIPANT_LEFT &&
            $event_type !== EventType::CALL_PARTICIPANT_DROPPED &&
            $event_type !== EventType::CALL_PARTICIPANT_DELETED
        ) {
            return;
        }

        if (rand(0, 1)) {
            /* HAKANA_FIXME[RedundantTruthinessCheck] Type true is always truthy */
            if ($event_type === EventType::CALL_PARTICIPANT_DROPPED && self::ENABLE_STALE_ATTENDEE_DELETION) {
                echo 'here';
            }

            /* HAKANA_FIXME[ImpossibleTruthinessCheck] Type false is never truthy */
            if ($event_type === EventType::CALL_PARTICIPANT_JOINED && self::ENABLE_STALE_ATTENDEE_DELETION_ON_REJOIN) {
                echo 'here';
            }
        }

        $are_all_attendees_deleted = \HH\Lib\C\count($a) === \HH\Lib\C\count($b) &&
            $event_type === EventType::CALL_PARTICIPANT_DELETED;

        echo $are_all_attendees_deleted;
    }
}


