import unittest

from quality import alignment_windows, make_words


class QualityTests(unittest.TestCase):
    def test_question_and_answer_keep_different_speakers(self):
        segments = [{"start": 380, "end": 383, "text": "You like the sports? I love football, jiu-jitsu.", "words": [
            {"word": "sports?", "start": 380.6, "end": 380.9, "score": 0.9},
            {"word": "I", "start": 381.1, "end": 381.2, "score": 0.9},
            {"word": "love", "start": 381.3, "end": 381.5, "score": 0.9}]}]
        words, count = make_words(segments, [(380, 381, "A"), (381, 383, "B")], 0, 400000)
        self.assertEqual([w["speaker"] for w in words], ["A", "B", "B"])
        self.assertEqual((words[0]["start_ms"], words[-1]["end_ms"]), (380600, 381500))
        self.assertEqual(count, 0)

    def test_overlapping_alignment_does_not_reorder_transcript(self):
        segments = [{"start": 1, "end": 4, "text": "The question?", "words": [
            {"word": "The", "start": 1, "end": 1.5},
            {"word": "question?", "start": 3, "end": 4}]},
            {"start": 2, "end": 3, "text": "Yeah.", "words": [
                {"word": "Yeah.", "start": 2, "end": 3}]}]
        words, _ = make_words(segments, [], 0, 10000)
        self.assertEqual([w["text"] for w in words], ["The", "question?", "Yeah."])

    def test_trim_offset_and_unknown_speaker(self):
        segments = [{"start": 0, "end": 1, "text": "Hello.", "words": [{"word": "Hello.", "start": 0.12, "end": 0.48}]}]
        words, _ = make_words(segments, [(10, 11, "A")], 33352, 34352)
        self.assertEqual(words[0]["start_ms"], 33472)
        self.assertEqual(words[0]["end_ms"], 33832)
        self.assertIsNone(words[0]["speaker"])

    def test_unaligned_text_is_preserved_without_fake_speaker(self):
        segments = [{"start": 1, "end": 2, "text": "£13.60", "words": [{"word": "£13.60"}]}]
        words, count = make_words(segments, [(0, 10, "A")], 0, 10000)
        self.assertEqual(words[0]["text"], "£13.60")
        self.assertEqual(words[0]["confidence"], 0)
        self.assertIsNone(words[0]["speaker"])
        self.assertEqual(count, 1)

    def test_alignment_has_context_but_stays_in_trim(self):
        windows = alignment_windows([
            {"start_ms": 33352, "end_ms": 35852, "text": "Question?"},
            {"start_ms": 35852, "end_ms": 36852, "text": "Yeah."}], 33352, 37000)
        self.assertEqual(len(windows), 1)
        self.assertEqual(windows[0]["text"], "Question? Yeah.")
        self.assertEqual(windows[0]["start"], 0)
        self.assertEqual(windows[0]["end"], 3.648)


if __name__ == "__main__":
    unittest.main()
