# Quizzes

This is the design for quizzing in StoryCraft. It follows how authors already think about quizzes in Storyline (question slides, feedback layers, a results slide) while keeping the engine rules explicit and testable.

## Concepts

- A **question slide** is an ordinary slide with a `question`. Its answer choices are ordinary objects on that slide, so authors can style and place them freely.
- **Feedback** is shown with ordinary layers: a question names its *correct*, *incorrect* and *try again* layers.
- **Results** are computed over every question slide in the course. A results slide is an ordinary slide that shows `%Results.…%` values and uses the "quiz passed" condition.

## Question types

| Type | Answer | Correct when |
|---|---|---|
| Multiple choice | one choice object | the selected choice is the correct one |
| True/false | one of two choice objects | as multiple choice |
| Multiple response | any number of choice objects | the selection is exactly the set of correct choices |
| Fill in the blank | typed text | it matches one of the accepted answers (trimmed; case-insensitive unless the question says otherwise) |
| Matching | each item paired with one match | every item is paired with its own match |

Choice objects behave like toggle buttons: in multiple choice and true/false, selecting one deselects the others; in multiple response each toggles. Selected choices are in the `Selected` state, so authors can style it.

## Submitting

The **Submit interaction** action evaluates the current slide's question:

1. With no answer, nothing is scored and the player tells the learner to answer first.
2. A correct answer earns the question's points, shows the *correct* layer, and locks the question.
3. An incorrect answer uses an attempt. With attempts left, the *try again* layer is shown (falling back to *incorrect*) and the learner can change their answer. With none left, the *incorrect* layer is shown and the question locks with 0 points.

`attempts = 0` means unlimited attempts. Locked questions ignore further selections.

## Results

- **Points:** the sum of points earned, out of the sum of all questions' points.
- **Percent:** points ÷ maximum × 100, rounded to a whole number.
- **Passed:** percent ≥ the course's passing percentage (default 80).

Text can show `%Results.ScorePoints%`, `%Results.MaxPoints%`, `%Results.ScorePercent%` and `%Results.PassPercent%`. Triggers can test the **quiz passed** condition.

Actions:

- **Submit results** reports the score to the LMS. If the course's completion rule is "quiz submitted", the course completes; the LMS also records passed or failed.
- **Reset results** clears every answer and attempt, so the learner can retake the quiz.

Answers and attempts are part of saved progress, so a learner who resumes keeps them.

## LMS reporting

| Format | Score | Pass/fail | Completion |
|---|---|---|---|
| SCORM 1.2 | `cmi.core.score.raw/min/max` (0–100) | `cmi.core.lesson_status` = `passed` / `failed` | as status |
| SCORM 2004 | `cmi.score.raw/min/max`, `cmi.score.scaled` (0–1) | `cmi.success_status` | `cmi.completion_status` |
| xAPI | statement `result.score` (`raw`, `min`, `max`, `scaled`) | verbs `passed` / `failed` | `completed` |
| cmi5 | `passed` / `failed` statements with `result.score.scaled` and `result.success`; mastery score from the launch data if the LMS sets one | | `completed` (moveOn) |

Per-question interactions (`cmi.interactions`, xAPI `answered`) are a later step.

## Status

- [x] Design (this page)
- [x] Model and engine: question types, selection, submit, attempts, feedback layers, results, retake, resume
- [ ] LMS score reporting (player)
- [ ] Editor: question slides, choice and feedback setup, results slide template
- [ ] Player: text entry and matching controls
- [ ] Storyline import of quiz slides (needs sample files)
