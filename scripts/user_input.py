

def get_string(prompt, default=None):
    user_in = input(prompt)
    if not user_in: return default
    return user_in

def get_option(prompt, options, default=None):
    while True:
        user_in = input(prompt)
        if not user_in:
            return default
        if user_in in options:
            return user_in

def get_num(prompt, default=None, min_val=None, max_val=None, dtype=float):
    while True:
        user_in = input(prompt)
        if not user_in: return default
        try:
            user_in = float(user_in)
        except ValueError:
            continue
        try:
            if dtype(user_in) == user_in:
                user_in = dtype(user_in)
        except ValueError:
            continue
        if min_val and user_in < min_val: continue
        if max_val and user_in > max_val: continue
        return user_in

def get_nums(prompt, default=None, min_val=None, max_val=None, min_n=None, max_n=None, dtype=float):
    while True:
        user_in = input(prompt)
        if not user_in: return default

        # split input
        user_in = user_in.replace('[', '').replace(']', '').replace('(', '').replace(')', '').replace(',', ' ').split()

        # check for n
        if min_n and len(user_in) < min_n: continue
        if max_n and len(user_in) > max_n: continue

        # convert to numeric and check range
        success = True
        for i in range(len(user_in)):
            try:
                user_in[i] = float(user_in[i])
            except ValueError:
                success = False
                break
            if min_val and user_in[i] < min_val: continue
            if max_val and user_in[i] > max_val: continue

        # convert to desired type (e.g. int)
        if not success: continue
        if dtype != float:
            for i in range(len(user_in)):
                try:
                    if dtype(user_in[i]) == user_in[i]:
                        user_in[i] = dtype(user_in[i])
                except ValueError:
                    success = False
                    break
            if not success: continue

        return user_in
        


