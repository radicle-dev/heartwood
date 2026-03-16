# Persona: Board Leader

## Goal: Structure boards workflow to the their culture

## Goal: Maintain the smallest set of boards to manage tasks at hand

## Goal: Focus boards with a goal

### Feature and scenario headings:

> Note that ✗ = unhappy paths and ✓ = happy paths, the order of items below should be shortest to longest path through each feature.

- Create a board
  - ✗ Repo not defined
  - ✗ Non-delegate
  - ✗ Empty
    - Name
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

- Show board
  - ✗ Repo not defined
  - ✗ Not found
  - ✗ Invalid sort
  - ✗ Invalid filter
  - ✗ Invalid assignee
  - ✓ Shown
    - Views:
      - Board view
        - No tasks
        - One task
        - Many tasks

        ```
        $ rad board show b67a123
        ╭──────────────────────────────────────────────────────────────────────────────╮
        │ Name     Engineering                                                         │
        │ Board    b67a123...                                                          │
        │ Goal     Ship V1                                                             │
        ├──────────────────────────────────────────────────────────────────────────────┤
        │ Todo                     │ Doing                    │ Done                   │
        ├──────────────────────────┼──────────────────────────┼────────────────────────┤
        │ ● 27d3132                │ ● 91f64c0                │ ● c10ecd2              │
        │   Change passphrase      │   Add CLI command for    │   git push: info links │
        │   prompt                 │   canonical refs         │   wrong                │
        │                          │                          │                        │
        │ ● 4e2a36e                │                          │ ● 23fef8f              │
        │   refs/heads/* cref      │                          │   Collect errors for   │
        │   rule conflicts         │                          │   fetch updates        │
        │                          │                          │                        │
        ╰──────────────────────────────────────────────────────────────────────────────╯
        ```

      - Table view
        - No tasks
        - One task
        - Many tasks

        ```
        $ rad board show b67a123 --table
        ╭─────────────────────────────────────────────────────────────────────────────────────╮
        │ ●    ID        Step    Title                  Assignee         Labels               │
        ├─────────────────────────────────────────────────────────────────────────────────────┤
        │ ●    27d3132   Todo    Change passphrase...   z6MkkPv...       bug                  │
        │ ●    91f64c0   Doing   Add CLI command...     z6MkkPv...       enhancement          │
        ╰─────────────────────────────────────────────────────────────────────────────────────╯
        ```

    - Filter
      - By step
        ```
        $ rad board show b67a123 --step "Todo"
        ...
        ```
      - By assignee
        ```
        $ rad board show b67a123 --assignee z6MkkPvad...
        ...
        ```
      - By author
        ```
        $ rad board show b67a123 --author adrianduke
        ...
        ```
      - By fuzzy search
        ```
        $ rad board show b67a123 --search "CLI"
        ...
        ```
    - Sort
      - By creation
        ```
        $ rad board show b67a123 --sort created
        ...
        ```
      - By updated
        ```
        $ rad board show b67a123 --sort updated
        ...
        ```
      - By assignee
        ```
        $ rad board show b67a123 --sort assignee
        ...
        ```
      - By author
        ```
        $ rad board show b67a123 --sort author
        ...
        ```
    - Archived
      ```
      $ rad board show b67a123 --archived
      ...
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
