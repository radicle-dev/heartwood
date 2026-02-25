# Persona: Board Leader

## Goal: Structure boards workflow to the their culture

## Goal: Maintain the smallest set of boards to manage tasks at hand

## Goal: Focus boards with a goal

### Feature and scenario headings:

- Create a board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Empty
    - Name
    - Goal
  - ✗ Invalid Characters
    - Name
    - Goal
  - ✗ Too Long
    - Name
    - Goal
  - ✗ Non-delegate can't create board
  - ✗ Name already exists
  - ✓ Delegate creates board
    - No Goal

    ```
    $ rad board create --name "Engineering"
    ✓ Board 'Engineering' created (b67a123)
    ```

    - With Goal

    ```
    $ rad board create --name "Engineering" --goal "Ship V1"
    ✓ Board 'Engineering' created (b67a123)
    ```
- Add step(s) to board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Empty
    - Label
  - ✗ Invalid Characters
    - Label
  - ✗ Too Long
    - Label
  - ✗ Label already exists
  - ✗ Label not found
    - Preceeding
    - Proceeding
    - Preceeding & Proceeding
  - ✓ Start step already exists; adds new start
    ```
    $ rad board step add b67a123 --label "Inbox" --start
    ✓ Step 'Inbox' added to board 'Engineering'
    ```
  - ✓ End step already exists; adds new end
    ```
    $ rad board step add b67a123 --label "Deployed" --end
    ✓ Step 'Deployed' added to board 'Engineering'
    ```
  - ✓ Nominate step
    - Start
    ```
    $ rad board step add b67a123 --label "Backlog" --start
    ✓ Step 'Backlog' added to board 'Engineering'
    ```

    - End
    ```
    $ rad board step add b67a123 --label "Done" --end
    ✓ Step 'Done' added to board 'Engineering'
    ```
- Remove step(s) from board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✗ Cannot remove nominated Start/End step (must re-nominate)
  - ✓ Empty start step
    ```
    $ rad board step rm b67a123 --step "Inbox"
    ✓ Step 'Inbox' removed from board 'Engineering'
    ```
  - ✓ Empty end step
    ```
    $ rad board step rm b67a123 --step "Deployed"
    ✓ Step 'Deployed' removed from board 'Engineering'
    ```
  - ✗ Non-empty step, no migrate
  - ✓ Non-empty step, with migrate
    ```
    $ rad board step rm b67a123 --step "Doing" --migrate-to "Todo"
    ✓ Step 'Doing' removed from board 'Engineering'
    ```
- Rename step(s)
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✗ Empty label
  - ✗ Invalid Label Characters
  - ✗ Label Too Long
  - ✗ New label already exists
  - ✓ Renamed
    ```
    $ rad board step edit b67a123 --step "Todo" --label "To Do"
    ✓ Step 'Todo' renamed to 'To Do'
    ```
- Nominate start and end steps
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✗ Start >= End
    - Start after End
    - End before Start
    - Start and End
  - ✓ Nominated
    - Start
    ```
    $ rad board step nominate b67a123 --start "Backlog"
    ✓ Board 'Engineering' updated
    ```

    - End
    ```
    $ rad board step nominate b67a123 --end "Done"
    ✓ Board 'Engineering' updated
    ```
- Reorder steps
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
    - Preceeding
    - Proceeding
    - Preceeding & Proceeding
  - ✗ Start >= End
    - Start after End
    - End before Start
    - Start and End
  - ✓ Reordered
    - To Start
    ```
    $ rad board step move b67a123 --step "Doing" --after "Backlog"
    ✓ Board 'Engineering' updated
    ```

    - To End
    ```
    $ rad board step move b67a123 --step "Review" --before "Done"
    ✓ Board 'Engineering' updated
    ```

    - Inbetween
    ```
    $ rad board step move b67a123 --step "Review" --after "Doing"
    ✓ Board 'Engineering' updated
    ```
- Add goal to board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Invalid goal characters
  - ✗ Goal too Long
  - ✗ Goal already exists
  - ✓ Added
    ```
    $ rad board edit b67a123 --goal "Ship V1"
    ✓ Board 'Engineering' updated
    ```
- Update board goal
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Invalid goal characters
  - ✗ Goal too Long
  - ✗ Goal already exists
  - ✓ Updated
    ```
    $ rad board edit b67a123 --goal "Ship V2"
    ✓ Board 'Engineering' updated
    ```
- Remove board goal
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✗ No existing goals
  - ✓ Removed
    ```
    $ rad board edit b67a123 --goal ""
    ✓ Board 'Engineering' updated
    ```
- Archive board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✓ Archived
    ```
    $ rad board archive b67a123
    ✓ Board 'Engineering' archived
    ```
- Un-archive board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Not found
  - ✓ Un-archived
    ```
    $ rad board unarchive b67a123
    ✓ Board 'Engineering' un-archived
    ```
