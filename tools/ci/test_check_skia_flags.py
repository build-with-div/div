import unittest

from tools.ci import check_skia_flags


class SkiaFlagsTest(unittest.TestCase):
    def test_a_list_with_the_flag_passes(self):
        text = 'A: extra_cflags += [ "-DX", "-USK_ENABLE_AVX512_OPTS", "-msse3" ]\n'
        self.assertEqual(check_skia_flags.problems_in("f", text), (1, []))

    def test_a_list_without_the_flag_fails_with_its_line(self):
        text = '\n\nA: extra_cflags += [ "-DX", "-msse3" ]\n'
        count, problems = check_skia_flags.problems_in("f", text)
        self.assertEqual(count, 1)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith("f:3: "))

    def test_one_bad_list_among_good_ones_fails(self):
        good = 'extra_cflags += [ "-USK_ENABLE_AVX512_OPTS" ]\n'
        count, problems = check_skia_flags.problems_in("f", good + 'extra_cflags += [ "-DX" ]\n')
        self.assertEqual((count, len(problems)), (2, 1))

    def test_a_list_across_lines_and_raw_strings_passes(self):
        text = (
            'concat!(r#"extra_cflags += [ "-DX", "#,\n'
            '    r#""-USK_ENABLE_AVX512_OPTS", "-msse3" ]"#)\n'
        )
        self.assertEqual(check_skia_flags.problems_in("f", text), (1, []))

    def test_defining_the_macro_fails(self):
        text = 'extra_cflags += [ "-USK_ENABLE_AVX512_OPTS", "-DSK_ENABLE_AVX512_OPTS" ]\n'
        _, problems = check_skia_flags.problems_in("f", text)
        self.assertEqual(len(problems), 1)

    def test_the_repository_passes(self):
        self.assertEqual(check_skia_flags.main(), 0)


if __name__ == "__main__":
    unittest.main()
